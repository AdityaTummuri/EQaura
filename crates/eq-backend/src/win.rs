use crate::{Action, Backend, Check, Device, Level, State, Status, apo, swap_file};
use std::path::PathBuf;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, STGM_READ,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};
use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RENDER_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render";
/// "Disable audio enhancements" for an endpoint.
const DISABLE_SYSFX: &str = "{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},5";
/// Equalizer APO's two effect classes.
const APO_GUIDS: [&str; 2] =
    ["{EACD2258-FCAC-4FF4-B36D-419E924A6D79}", "{EC1CC9CE-FAED-4822-828A-82A81A6F018F}"];
const DOWNLOAD: &str = "https://sourceforge.net/projects/equalizerapo/";

pub struct Windows {
    enumerator: Option<IMMDeviceEnumerator>,
    last: Option<String>,
    enabled: bool,
    error: Option<String>,
}

// The enumerator is created in the multithreaded apartment and only used by
// the thread that owns the backend.
unsafe impl Send for Windows {}

struct Apo {
    install: PathBuf,
    config: PathBuf,
}

fn find_apo() -> Option<Apo> {
    let key = LOCAL_MACHINE.open(r"SOFTWARE\EqualizerAPO").ok()?;
    let install = PathBuf::from(key.get_string("InstallPath").ok()?);
    let config = key.get_string("ConfigPath").map(PathBuf::from).unwrap_or_else(|_| install.join("config"));
    Some(Apo { install, config })
}

/// How Equalizer APO is attached to an endpoint.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hook {
    Active,
    EnhancementsOff,
    /// Set up once, then removed, usually by a driver or Windows update.
    Lost,
    Never,
}

fn hook(guid: &str) -> Hook {
    let configured = LOCAL_MACHINE.open(format!(r"SOFTWARE\EqualizerAPO\Child APOs\{guid}")).is_ok();
    let Ok(fx) = LOCAL_MACHINE.open(format!(r"{RENDER_KEY}\{guid}\FxProperties")) else {
        return if configured { Hook::Lost } else { Hook::Never };
    };
    let mut attached = false;
    if let Ok(values) = fx.values() {
        for (name, value) in values {
            let single = String::try_from(value.clone()).ok();
            let many = fx.get_multi_string(&name).ok();
            let hit = |s: &String| APO_GUIDS.iter().any(|g| s.eq_ignore_ascii_case(g));
            attached |= single.as_ref().is_some_and(hit) || many.is_some_and(|m| m.iter().any(hit));
        }
    }
    let off = fx.get_u32(DISABLE_SYSFX).is_ok_and(|v| v == 1);
    match (attached, off, configured) {
        (true, true, _) => Hook::EnhancementsOff,
        (true, false, _) => Hook::Active,
        (false, _, true) => Hook::Lost,
        (false, _, false) => Hook::Never,
    }
}

impl Windows {
    pub fn new(_dir: PathBuf) -> Self {
        // SAFETY: plain COM setup. A second call on this thread is harmless.
        let enumerator = unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()
        };
        Self { enumerator, last: None, enabled: false, error: None }
    }

    fn id(device: &IMMDevice) -> Option<String> {
        // SAFETY: the returned string is ours to read once and free.
        unsafe {
            let p = device.GetId().ok()?;
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            s
        }
    }

    fn name(device: &IMMDevice) -> Option<String> {
        // SAFETY: the property store outlives the value read from it.
        unsafe {
            let store = device.OpenPropertyStore(STGM_READ).ok()?;
            let value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
            let s = value.to_string();
            (!s.is_empty()).then_some(s)
        }
    }

    fn apply_inner(&mut self, state: &State) -> Result<(), String> {
        let Some(apo) = find_apo() else {
            return if state.enabled { Err("Equalizer APO is not installed.".into()) } else { Ok(()) };
        };
        let text = apo::config(state);
        let file = apo.config.join(apo::FILE);
        if self.last.as_deref() != Some(text.as_str()) || !file.exists() {
            swap_file(&file, text.as_bytes())
                .map_err(|e| format!("Could not write Equalizer APO's settings: {e}"))?;
            self.last = Some(text);
        }
        Ok(())
    }

    fn add_include(only: bool) -> Result<String, String> {
        let apo = find_apo().ok_or("Equalizer APO is not installed.")?;
        let path = apo.config.join("config.txt");
        let old = std::fs::read(&path).unwrap_or_default();
        let new = if only {
            let backup = apo.config.join("config.before-eqaura.txt");
            if !backup.exists() {
                std::fs::write(&backup, &old).map_err(|e| format!("Could not back up the settings: {e}"))?;
            }
            format!("{}\r\n", apo::INCLUDE).into_bytes()
        } else {
            apo::with_include(&old)
        };
        swap_file(&path, &new).map_err(|e| format!("Could not change Equalizer APO's settings: {e}"))?;
        Ok(if only {
            "Only EQaura is active now. The old settings are in config.before-eqaura.txt".into()
        } else {
            "EQaura is connected to Equalizer APO".into()
        })
    }
}

impl Backend for Windows {
    fn devices(&mut self) -> Vec<Device> {
        let Some(e) = &self.enumerator else { return Vec::new() };
        // SAFETY: standard Core Audio enumeration on the owning thread.
        unsafe {
            let default = e.GetDefaultAudioEndpoint(eRender, eConsole).ok().as_ref().and_then(Self::id);
            let Ok(list) = e.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return Vec::new() };
            (0..list.GetCount().unwrap_or(0))
                .filter_map(|i| list.Item(i).ok())
                .filter_map(|d| {
                    let id = Self::id(&d)?;
                    let name = Self::name(&d).unwrap_or_else(|| "Output device".into());
                    let is_default = default.as_ref() == Some(&id);
                    Some(Device { id, name, is_default })
                })
                .collect()
        }
    }

    fn apply(&mut self, state: &State) -> Result<(), String> {
        let r = self.apply_inner(state);
        self.enabled = state.enabled;
        self.error = r.clone().err();
        r
    }

    fn status(&mut self) -> Status {
        if find_apo().is_none() {
            return Status::new(Level::Bad, "Equalizer APO is missing");
        }
        if let Some(e) = &self.error {
            return Status::new(Level::Bad, e);
        }
        if !self.enabled {
            return Status::new(Level::Info, "Off");
        }
        let device = self.devices().into_iter().find(|d| d.is_default);
        match device.as_ref().and_then(|d| apo::endpoint_guid(&d.id)).map(hook) {
            Some(Hook::Active) => Status::new(Level::Good, "Active"),
            Some(Hook::EnhancementsOff) => Status::new(Level::Warning, "Enhancements are off"),
            Some(Hook::Lost) => Status::new(Level::Warning, "Needs repair"),
            Some(Hook::Never) => Status::new(Level::Warning, "Device not set up"),
            None => Status::new(Level::Info, "No output device"),
        }
    }

    fn checks(&mut self) -> Vec<Check> {
        let mut out = Vec::new();
        let Some(apo) = find_apo() else {
            out.push(
                Check::new(
                    Level::Bad,
                    "Equalizer APO is not installed",
                    "EQaura uses it to process sound. Install it, tick your output devices, and restart if asked.",
                )
                .with("Download", Action::OpenLink(DOWNLOAD)),
            );
            return out;
        };
        out.push(Check::new(Level::Good, "Equalizer APO is installed", &apo.install.to_string_lossy()));
        let main = apo::inspect(&std::fs::read(apo.config.join("config.txt")).unwrap_or_default());
        if main.has_include {
            out.push(Check::new(Level::Good, "EQaura is connected", "Equalizer APO loads EQaura's settings."));
        } else {
            out.push(
                Check::new(Level::Bad, "EQaura is not connected", "Equalizer APO does not load EQaura's settings yet.")
                    .with("Connect", Action::AddInclude),
            );
        }
        if !main.others.is_empty() {
            out.push(
                Check::new(
                    Level::Warning,
                    "Other Equalizer APO settings are active",
                    &format!("They change the sound as well: {}", main.others.join(" | ")),
                )
                .with("Use only EQaura", Action::UseOnlyEqaura),
            );
        }
        for d in self.devices() {
            let Some(h) = apo::endpoint_guid(&d.id).map(hook) else { continue };
            let check = match h {
                Hook::Active => Check::new(Level::Good, &d.name, "The equalizer works on this device."),
                Hook::EnhancementsOff => Check::new(
                    Level::Warning,
                    &d.name,
                    "Audio enhancements are turned off for this device. Turn them on in Windows sound settings, or repair it.",
                )
                .with("Repair", Action::OpenDeviceSelector),
                Hook::Lost => Check::new(
                    Level::Warning,
                    &d.name,
                    "A driver or Windows update removed the equalizer from this device.",
                )
                .with("Repair", Action::OpenDeviceSelector),
                Hook::Never => Check::new(
                    if d.is_default { Level::Warning } else { Level::Info },
                    &d.name,
                    "Not set up. Tick this device in Equalizer APO's device list.",
                )
                .with("Set up", Action::OpenDeviceSelector),
            };
            out.push(check);
        }
        if let Some(e) = &self.error {
            out.push(Check::new(Level::Bad, "Last problem", e));
        }
        out
    }

    fn act(&mut self, action: Action) -> Result<String, String> {
        match action {
            Action::OpenLink(l) => crate::open_path(l).map(|()| "Opened in your browser".into()),
            Action::AddInclude => Self::add_include(false),
            Action::UseOnlyEqaura => Self::add_include(true),
            Action::RestartEngine => {
                self.last = None;
                Ok("Settings written again".into())
            }
            Action::OpenDeviceSelector => {
                let apo = find_apo().ok_or("Equalizer APO is not installed.")?;
                // Named Configurator.exe before version 1.4.
                let exe = ["DeviceSelector.exe", "Configurator.exe"]
                    .iter()
                    .map(|n| apo.install.join(n))
                    .find(|p| p.exists())
                    .ok_or("Equalizer APO's device list was not found. Reinstall Equalizer APO.")?;
                let exe = HSTRING::from(exe.as_os_str());
                // SAFETY: both strings live until the call returns.
                let r = unsafe {
                    ShellExecuteW(None, w!("runas"), &exe, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL)
                };
                // Values above 32 mean success.
                if r.0 as usize > 32 {
                    Ok("Tick your output devices in the window that opened".into())
                } else {
                    Err("The device list was not opened.".into())
                }
            }
            Action::OpenPrivacySettings => Err("Not needed on Windows.".into()),
        }
    }

    fn supports_start_at_login(&self) -> bool {
        true
    }

    /// Starts the small tray helper, not the app. The sound needs neither.
    fn set_start_at_login(&mut self, on: bool) -> Result<(), String> {
        let key = CURRENT_USER.create(RUN_KEY).map_err(|e| e.message())?;
        if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let agent = exe.with_file_name("eqaura-agent.exe");
            key.set_string("EQaura", format!("\"{}\"", agent.display())).map_err(|e| e.message())
        } else {
            match key.remove_value("EQaura") {
                Ok(()) => Ok(()),
                // Already gone.
                Err(_) => Ok(()),
            }
        }
    }
}
