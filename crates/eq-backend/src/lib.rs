//! One interface over the three audio engines: Equalizer APO on Windows,
//! PipeWire on Linux and Core Audio on macOS.

pub mod apo;
pub mod pipewire;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
pub mod mac;
#[cfg(windows)]
mod win;

use eq_core::profile::Profile;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Device {
    /// Stable across restarts.
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Good,
    Info,
    Warning,
    Bad,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub level: Level,
    pub text: String,
}

impl Status {
    pub fn new(level: Level, text: &str) -> Self {
        Self { level, text: text.into() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    OpenLink(&'static str),
    OpenDeviceSelector,
    AddInclude,
    UseOnlyEqaura,
    RestartEngine,
    OpenPrivacySettings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub level: Level,
    pub title: String,
    pub detail: String,
    pub action: Option<(String, Action)>,
}

impl Check {
    pub fn new(level: Level, title: &str, detail: &str) -> Self {
        Self { level, title: title.into(), detail: detail.into(), action: None }
    }

    pub fn with(mut self, label: &str, action: Action) -> Self {
        self.action = Some((label.into(), action));
        self
    }
}

/// Everything an engine needs to know.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub enabled: bool,
    /// Sound per device id. Devices that are not listed stay untouched.
    pub devices: BTreeMap<String, Profile>,
}

pub trait Backend: Send {
    fn devices(&mut self) -> Vec<Device>;
    /// Makes `state` audible. Called often while dragging.
    fn apply(&mut self, state: &State) -> Result<(), String>;
    fn status(&mut self) -> Status;
    fn checks(&mut self) -> Vec<Check>;
    /// Performs a repair; returns a short message for the user.
    fn act(&mut self, action: Action) -> Result<String, String>;
    fn supports_start_at_login(&self) -> bool;
    fn set_start_at_login(&mut self, on: bool) -> Result<(), String>;
}

/// The backend for this computer. `data_dir` is EQaura's own folder.
pub fn system(data_dir: PathBuf) -> Box<dyn Backend> {
    #[cfg(windows)]
    return Box::new(win::Windows::new(data_dir));
    #[cfg(target_os = "macos")]
    return Box::new(mac::Mac::new(data_dir));
    #[cfg(target_os = "linux")]
    return Box::new(linux::Linux::new(data_dir));
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    return Box::new(Demo::new(data_dir));
}

/// Pretends to be an engine. Used for screenshots, demos and tests.
pub struct Demo {
    pub last: State,
}

impl Demo {
    pub fn new(_data_dir: PathBuf) -> Self {
        Self { last: State::default() }
    }
}

impl Backend for Demo {
    fn devices(&mut self) -> Vec<Device> {
        vec![
            Device { id: "demo-headphones".into(), name: "Headphones".into(), is_default: true },
            Device { id: "demo-speakers".into(), name: "Built-in speakers".into(), is_default: false },
        ]
    }

    fn apply(&mut self, state: &State) -> Result<(), String> {
        self.last = state.clone();
        Ok(())
    }

    fn status(&mut self) -> Status {
        if self.last.enabled { Status::new(Level::Good, "Active") } else { Status::new(Level::Info, "Off") }
    }

    fn checks(&mut self) -> Vec<Check> {
        vec![Check::new(Level::Info, "Demo mode", "Nothing is changed on this computer.")]
    }

    fn act(&mut self, _: Action) -> Result<String, String> {
        Ok("Done".into())
    }

    fn supports_start_at_login(&self) -> bool {
        false
    }

    fn set_start_at_login(&mut self, _: bool) -> Result<(), String> {
        Ok(())
    }
}

/// Opens a web page or folder with the system's default app.
pub fn open_path(target: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let cmd = std::process::Command::new("open").arg(target).spawn();
    #[cfg(windows)]
    let cmd = std::process::Command::new("explorer").arg(target).spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let cmd = std::process::Command::new("xdg-open").arg(target).spawn();
    match cmd {
        Ok(mut child) => {
            // Reap it in the background so it never becomes a zombie.
            std::thread::spawn(move || child.wait());
            Ok(())
        }
        Err(e) => Err(format!("Could not open it: {e}")),
    }
}

/// Replaces a file in one step. Not synced to disk: these files are rewritten
/// many times a second while dragging.
#[allow(dead_code)]
pub(crate) fn swap_file(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
