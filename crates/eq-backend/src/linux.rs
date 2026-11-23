use crate::pipewire::{self as pw, Graph, Routing, Slots};
use crate::{Action, Backend, Check, Device, Level, State, Status, swap_file};
use eq_core::profile::Profile;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const UNIT: &str = "eqaura.service";

pub struct Linux {
    dir: PathBuf,
    running: Option<Running>,
    child: Option<Child>,
    /// Default output before EQaura took its place (WirePlumber 0.4).
    real_default: Option<String>,
    version: Option<(u32, u32, u32)>,
    wireplumber: Option<(u32, u32, u32)>,
    systemd: bool,
    graph: (Graph, Option<Instant>),
    error: Option<String>,
    enabled: bool,
}

struct Running {
    slots: Slots,
    rate: u32,
    routing: Routing,
}

fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        Ok(text)
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("{cmd}: {}", err.lines().next().unwrap_or("failed")))
    }
}

impl Linux {
    pub fn new(dir: PathBuf) -> Self {
        let version = run("pipewire", &["--version"]).ok().as_deref().and_then(pw::read_version);
        let wireplumber = run("wireplumber", &["--version"]).ok().as_deref().and_then(pw::read_version);
        let systemd = run("systemctl", &["--user", "show", "-p", "Version"]).is_ok();
        Self {
            dir,
            running: None,
            child: None,
            real_default: None,
            version,
            wireplumber,
            systemd,
            graph: (Graph::default(), None),
            error: None,
            enabled: false,
        }
    }

    fn graph(&mut self, max_age: Duration) -> Graph {
        let fresh = self.graph.1.is_some_and(|t| t.elapsed() < max_age);
        if !fresh && let Ok(json) = run("pw-dump", &[]) {
            self.graph = (pw::read_dump(&json), Some(Instant::now()));
        }
        self.graph.0.clone()
    }

    fn smart(&self) -> bool {
        self.wireplumber.is_some_and(|v| v >= (0, 5, 0))
    }

    /// The real output device, never our own sink.
    fn real_sink(&mut self, g: &Graph) -> Option<String> {
        let exists = |n: &String| g.sinks.iter().any(|s| s.1 == *n);
        let current = g.default_sink.clone().filter(|n| n != pw::SINK && exists(n));
        if current.is_some() {
            self.real_default = current;
        }
        self.real_default
            .clone()
            .filter(exists)
            .or_else(|| g.sinks.iter().find(|s| s.1 != pw::SINK).map(|s| s.1.clone()))
    }

    fn config_path(&self) -> PathBuf {
        self.dir.join("pipewire-eqaura.conf")
    }

    fn unit_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("systemd/user").join(UNIT))
    }

    fn start(&mut self, slots: Slots, rate: u32, routing: Routing) -> Result<(), String> {
        let path = self.config_path();
        swap_file(&path, pw::config(&slots, rate, &routing).as_bytes())
            .map_err(|e| format!("Could not write the PipeWire settings: {e}"))?;
        let path = path.to_string_lossy().into_owned();
        if self.systemd {
            let unit = Self::unit_path().ok_or("The home folder was not found.")?;
            let text = pw::unit(&path);
            if std::fs::read_to_string(&unit).ok().as_deref() != Some(text.as_str()) {
                swap_file(&unit, text.as_bytes()).map_err(|e| format!("Could not write the service: {e}"))?;
                run("systemctl", &["--user", "daemon-reload"])?;
            }
            run("systemctl", &["--user", "enable", UNIT])?;
            let _ = run("systemctl", &["--user", "reset-failed", UNIT]);
            run("systemctl", &["--user", "restart", UNIT])?;
        } else {
            self.kill_child();
            let child = Command::new("pipewire")
                .args(["-c", &path])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("PipeWire could not start: {e}"))?;
            self.child = Some(child);
        }
        // Wait for the sink to appear before routing sound to it.
        let mut id = None;
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(50));
            let g = self.graph(Duration::ZERO);
            id = g.id_of(pw::SINK);
            if id.is_some() {
                break;
            }
        }
        let id = id.ok_or("PipeWire did not start the equalizer. See the System page.")?;
        if !routing.smart {
            run("wpctl", &["set-default", &id.to_string()])?;
        }
        self.running = Some(Running { slots, rate, routing });
        Ok(())
    }

    fn kill_child(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        self.running = None;
        self.kill_child();
        let mut result = Ok(());
        if self.systemd && Self::unit_path().is_some_and(|p| p.exists()) {
            result = run("systemctl", &["--user", "disable", "--now", UNIT]).map(|_| ());
        }
        // Give the output back to the real device.
        let g = self.graph(Duration::ZERO);
        if let Some(id) = self.real_default.clone().and_then(|n| g.id_of(&n))
            && g.default_sink.as_deref().is_none_or(|d| d == pw::SINK)
        {
            let _ = run("wpctl", &["set-default", &id.to_string()]);
        }
        result
    }

    fn apply_inner(&mut self, state: &State) -> Result<(), String> {
        if !state.enabled {
            return if self.running.is_some() || self.enabled || self.service_active() { self.stop() } else { Ok(()) };
        }
        if self.version.is_none() {
            return Err("PipeWire was not found on this computer.".into());
        }
        let age = if self.running.is_some() { Duration::from_millis(1500) } else { Duration::ZERO };
        let g = self.graph(age);
        let target = self.real_sink(&g);
        let flat = Profile::default();
        let profile = target.as_ref().and_then(|t| state.devices.get(t)).unwrap_or(&flat);
        let rate = g.rate.unwrap_or(48_000);
        let chain = profile.chain(f64::from(rate));
        let routing = Routing { smart: self.smart(), target: if self.smart() { None } else { target } };

        if let Some(r) = &self.running
            && r.rate == rate
            && r.routing == routing
            && r.slots.len() >= chain.left.len().max(chain.right.len()) + 1
            && let Some(id) = g.id_of(pw::SINK)
        {
            let new = pw::slots(&chain, f64::from(rate), r.slots.len());
            let id = id.to_string();
            let mut ok = true;
            for params in pw::live_updates(&r.slots, &new) {
                ok &= run("pw-cli", &["set-param", &id, "Props", &params]).is_ok();
            }
            if ok {
                // Also on disk, so the same sound comes back after a restart.
                let text = pw::config(&new, rate, &routing);
                swap_file(&self.config_path(), text.as_bytes())
                    .map_err(|e| format!("Could not write the PipeWire settings: {e}"))?;
                if let Some(r) = &mut self.running {
                    r.slots = new;
                }
                return Ok(());
            }
        }
        let slots = pw::slots(&chain, f64::from(rate), pw::slots_for(&chain));
        self.start(slots, rate, routing)
    }

    fn service_active(&self) -> bool {
        self.systemd && run("systemctl", &["--user", "is-active", UNIT]).is_ok()
    }
}

impl Backend for Linux {
    fn devices(&mut self) -> Vec<Device> {
        let g = self.graph(Duration::from_millis(1500));
        let real = self.real_sink(&g);
        g.sinks
            .iter()
            .filter(|s| s.1 != pw::SINK)
            .map(|s| Device { id: s.1.clone(), name: s.2.clone(), is_default: real.as_ref() == Some(&s.1) })
            .collect()
    }

    fn apply(&mut self, state: &State) -> Result<(), String> {
        let r = self.apply_inner(state);
        self.enabled = state.enabled;
        self.error = r.clone().err();
        r
    }

    fn status(&mut self) -> Status {
        if let Some(e) = &self.error {
            return Status::new(Level::Bad, e);
        }
        if !self.enabled {
            return Status::new(Level::Info, "Off");
        }
        let g = self.graph(Duration::from_millis(1500));
        if g.id_of(pw::SINK).is_some() {
            Status::new(Level::Good, "Active")
        } else {
            Status::new(Level::Bad, "Engine stopped")
        }
    }

    fn checks(&mut self) -> Vec<Check> {
        let mut out = Vec::new();
        match self.version {
            None => {
                out.push(
                    Check::new(
                        Level::Bad,
                        "PipeWire is not installed",
                        "EQaura needs PipeWire 1.0 or newer. Older systems that use only PulseAudio are not supported.",
                    )
                    .with("How to install", Action::OpenLink("https://pipewire.org")),
                );
                return out;
            }
            Some(v) if v < (1, 0, 0) => out.push(Check::new(
                Level::Warning,
                &format!("PipeWire {}.{}.{} is old", v.0, v.1, v.2),
                "EQaura is made for PipeWire 1.0 or newer. Some things may not work.",
            )),
            Some(v) => out.push(Check::new(
                Level::Good,
                &format!("PipeWire {}.{}.{}", v.0, v.1, v.2),
                "The sound server is ready.",
            )),
        }
        if self.smart() {
            out.push(Check::new(
                Level::Good,
                "Follows your output device",
                "WirePlumber moves the equalizer when you switch outputs.",
            ));
        } else {
            out.push(Check::new(
                Level::Info,
                "Output switching while EQaura is closed",
                "This WirePlumber version cannot move the equalizer by itself. Keep EQaura open when you switch outputs, or choose EQaura as the output.",
            ));
        }
        for tool in ["pw-dump", "pw-cli", "wpctl"] {
            if run(tool, &["--version"]).is_err() && run(tool, &["--help"]).is_err() {
                out.push(Check::new(
                    Level::Bad,
                    &format!("{tool} is missing"),
                    "Install your system's PipeWire and WirePlumber tools.",
                ));
            }
        }
        if !self.systemd {
            out.push(Check::new(
                Level::Info,
                "The equalizer stops when EQaura closes",
                "This system has no systemd user services, which keep it running in the background.",
            ));
        }
        if self.enabled {
            let g = self.graph(Duration::ZERO);
            if g.id_of(pw::SINK).is_some() {
                out.push(Check::new(Level::Good, "The equalizer is running", "It keeps working when EQaura is closed."));
            } else {
                out.push(
                    Check::new(Level::Bad, "The equalizer is not running", "Restart it to hear your settings again.")
                        .with("Restart", Action::RestartEngine),
                );
            }
        }
        if let Some(e) = &self.error {
            out.push(Check::new(Level::Bad, "Last problem", e).with("Try again", Action::RestartEngine));
        }
        out
    }

    fn act(&mut self, action: Action) -> Result<String, String> {
        match action {
            Action::OpenLink(l) => crate::open_path(l).map(|()| "Opened in your browser".into()),
            Action::RestartEngine => {
                // The next apply starts it again from scratch.
                self.running = None;
                self.error = None;
                Ok("Restarting the equalizer".into())
            }
            _ => Err("Not available on Linux.".into()),
        }
    }

    fn supports_start_at_login(&self) -> bool {
        false
    }

    fn set_start_at_login(&mut self, _: bool) -> Result<(), String> {
        Ok(())
    }
}
