//! macOS: a small background process captures system sound with a Core Audio
//! tap (macOS 14.2 or newer), filters it and plays it to the output device.
//! It runs apart from the window, so closing or crashing the app never
//! interrupts playback, and if it stops, macOS plays the sound unchanged.

mod audio;
mod engine;
mod rt;

pub use engine::run as run_engine;

use crate::{Action, Backend, Check, Device, Level, State, Status, swap_file};
use serde::{Deserialize, Serialize};
use std::os::unix::net::UnixDatagram;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const AGENT_LABEL: &str = "io.github.adityatummuri.eqaura.engine";
pub const ENGINE_FLAG: &str = "--engine";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    /// No settings for the current device, so the tap is not needed.
    Flat,
    /// Listening for the first sound, which also proves permission is given.
    Waiting,
    Active,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Report {
    pub pid: u32,
    pub version: String,
    pub phase: Phase,
    pub detail: String,
}

/// Socket paths are limited to about 100 bytes, so the folder must be short.
pub(crate) fn socket_path(dir: &Path) -> PathBuf {
    let p = dir.join("engine.sock");
    if p.as_os_str().len() < 100 { p } else { std::env::temp_dir().join("eqaura-engine.sock") }
}

pub(crate) fn state_path(dir: &Path) -> PathBuf {
    dir.join("engine.json")
}

pub(crate) fn report_path(dir: &Path) -> PathBuf {
    dir.join("engine-status.json")
}

pub struct Mac {
    dir: PathBuf,
    last: Option<State>,
    spawned: Option<Instant>,
    failed_starts: u32,
    error: Option<String>,
}

fn os_version() -> (u32, u32) {
    let out = Command::new("/usr/bin/sw_vers").arg("-productVersion").output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let mut p = text.trim().split('.').map(|n| n.parse().unwrap_or(0));
    (p.next().unwrap_or(0), p.next().unwrap_or(0))
}

impl Mac {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, last: None, spawned: None, failed_starts: 0, error: None }
    }

    /// True when the engine process received the message.
    fn send(&self, message: &[u8]) -> bool {
        UnixDatagram::unbound().and_then(|s| s.send_to(message, socket_path(&self.dir))).is_ok()
    }

    fn report(&self) -> Option<Report> {
        let text = std::fs::read_to_string(report_path(&self.dir)).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn spawn(&mut self) -> Result<(), String> {
        // Give a new process time to open its socket before trying again.
        if self.spawned.is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
            return Ok(());
        }
        if self.failed_starts >= 5 {
            return Err("The sound engine keeps stopping. See the System page.".into());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let child = Command::new(exe)
            .arg(ENGINE_FLAG)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            // Its own group, so it keeps running after the app quits.
            .process_group(0)
            .spawn()
            .map_err(|e| format!("The sound engine could not start: {e}"))?;
        std::thread::spawn(move || {
            let mut child = child;
            child.wait()
        });
        self.failed_starts += 1;
        self.spawned = Some(Instant::now());
        Ok(())
    }

    fn apply_inner(&mut self, state: &State) -> Result<(), String> {
        let changed = self.last.as_ref() != Some(state);
        if changed {
            let json = serde_json::to_vec(state).map_err(|e| e.to_string())?;
            swap_file(&state_path(&self.dir), &json)
                .map_err(|e| format!("Could not write the engine settings: {e}"))?;
            self.last = Some(state.clone());
        }
        // After an update, let the old engine go and start the new one.
        let old = self.report().is_some_and(|r| r.version != env!("CARGO_PKG_VERSION"));
        if old && self.send(b"quit") {
            std::thread::sleep(Duration::from_millis(400));
            self.spawned = None;
        }
        let alive = self.send(if changed { b"reload" } else { b"ping" });
        if alive {
            self.failed_starts = 0;
            Ok(())
        } else if state.enabled {
            self.spawn()
        } else {
            Ok(())
        }
    }

    fn agent_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join(format!("Library/LaunchAgents/{AGENT_LABEL}.plist")))
    }
}

impl Backend for Mac {
    fn devices(&mut self) -> Vec<Device> {
        audio::devices()
    }

    fn apply(&mut self, state: &State) -> Result<(), String> {
        let r = self.apply_inner(state);
        self.error = r.clone().err();
        r
    }

    fn status(&mut self) -> Status {
        if let Some(e) = &self.error {
            return Status::new(Level::Bad, e);
        }
        if !self.last.as_ref().is_some_and(|s| s.enabled) {
            return Status::new(Level::Info, "Off");
        }
        if !self.send(b"ping") {
            return Status::new(Level::Info, "Starting");
        }
        match self.report().map(|r| r.phase) {
            Some(Phase::Active) => Status::new(Level::Good, "Active"),
            Some(Phase::Flat) => Status::new(Level::Good, "Ready"),
            Some(Phase::Waiting) => Status::new(Level::Info, "Waiting for sound"),
            Some(Phase::Failed) => Status::new(Level::Bad, "Engine problem"),
            None => Status::new(Level::Info, "Starting"),
        }
    }

    fn checks(&mut self) -> Vec<Check> {
        let mut out = Vec::new();
        let v = os_version();
        out.push(if v >= (14, 2) {
            Check::new(
                Level::Good,
                &format!("macOS {}.{}", v.0, v.1),
                "This version can capture system sound.",
            )
        } else {
            Check::new(Level::Bad, &format!("macOS {}.{}", v.0, v.1), "EQaura needs macOS 14.2 or newer.")
        });
        let enabled = self.last.as_ref().is_some_and(|s| s.enabled);
        let alive = self.send(b"ping");
        match (enabled, alive, self.report()) {
            (false, ..) => {
                out.push(Check::new(Level::Info, "The equalizer is off", "Turn it on at the top of the window."));
            }
            (true, false, _) => out.push(
                Check::new(Level::Bad, "The sound engine is not running", "Restart it to hear your settings.")
                    .with("Restart", Action::RestartEngine),
            ),
            (true, true, Some(r)) => match r.phase {
                Phase::Active => out.push(Check::new(
                    Level::Good,
                    "The sound engine is active",
                    "It keeps working when this window is closed.",
                )),
                Phase::Flat => out.push(Check::new(
                    Level::Good,
                    "The sound engine is ready",
                    "This device has no changes, so sound plays untouched.",
                )),
                Phase::Waiting => out.push(
                    Check::new(
                        Level::Info,
                        "Waiting for sound",
                        "Play something. If this stays here while music plays, allow EQaura under System Audio Recording. Until then you hear the normal sound.",
                    )
                    .with("Open privacy settings", Action::OpenPrivacySettings),
                ),
                Phase::Failed => out.push(
                    Check::new(Level::Bad, "The sound engine has a problem", &r.detail)
                        .with("Restart", Action::RestartEngine),
                ),
            },
            (true, true, None) => out.push(Check::new(Level::Info, "The sound engine is starting", "")),
        }
        if !std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/")) {
            out.push(Check::new(
                Level::Info,
                "Running outside the app bundle",
                "macOS asks for permission in the name of the program that started EQaura, such as Terminal.",
            ));
        }
        if let Some(e) = &self.error {
            out.push(Check::new(Level::Bad, "Last problem", e).with("Try again", Action::RestartEngine));
        }
        out
    }

    fn act(&mut self, action: Action) -> Result<String, String> {
        match action {
            Action::OpenLink(l) => crate::open_path(l).map(|()| "Opened in your browser".into()),
            Action::OpenPrivacySettings => crate::open_path(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_AudioCapture",
            )
            .map(|()| "Allow EQaura under System Audio Recording".into()),
            Action::RestartEngine => {
                self.send(b"quit");
                std::thread::sleep(Duration::from_millis(400));
                self.spawned = None;
                self.failed_starts = 0;
                self.error = None;
                if let Some(state) = self.last.clone() {
                    self.apply(&state)?;
                }
                Ok("Restarting the sound engine".into())
            }
            _ => Err("Not needed on macOS.".into()),
        }
    }

    fn supports_start_at_login(&self) -> bool {
        true
    }

    fn set_start_at_login(&mut self, on: bool) -> Result<(), String> {
        let path = Self::agent_path().ok_or("The home folder was not found.")?;
        if !on {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.to_string_lossy().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{AGENT_LABEL}</string>
<key>ProgramArguments</key><array><string>{exe}</string><string>{ENGINE_FLAG}</string></array>
<key>RunAtLoad</key><true/>
<key>ProcessType</key><string>Interactive</string>
</dict></plist>
"#
        );
        swap_file(&path, plist.as_bytes()).map_err(|e| e.to_string())
    }
}
