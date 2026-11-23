//! The background process: follows the default output device and settings.

use super::audio::{self, Aggregate, Tap, check};
use super::rt::{Rt, Shared};
use super::{Phase, Report, report_path, socket_path, state_path};
use crate::{State, swap_file};
use eq_core::dsp::Stereo;
use eq_core::profile::Chain;
use objc2_core_audio::{
    AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop,
    AudioObjectID,
};
use objc2_core_audio_types::{AudioBufferList, AudioTimeStamp};
use rtrb::{Consumer, Producer, RingBuffer};
use std::ffi::c_void;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

unsafe extern "C-unwind" fn io_proc(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    // SAFETY: `client` is the Rt owned by the running Session, which stops IO
    // before freeing it. Core Audio never runs two cycles at once.
    unsafe {
        if let Some(rt) = client.cast::<Rt>().as_mut() {
            rt.cycle(input.as_ptr(), output.as_ptr());
        }
    }
    0
}

/// A running tap. Fields drop in order: IO stops first, then the device, then
/// the tap, after which macOS plays sound normally.
struct Session {
    io: Io,
    _aggregate: Aggregate,
    _tap: Tap,
    device: AudioObjectID,
    rate: f64,
    audible: bool,
    chain: Chain,
    send: Producer<Box<Stereo>>,
    retired: Consumer<Box<Stereo>>,
    pending: Option<Chain>,
    shared: Arc<Shared>,
    cycles: (u64, Instant),
}

struct Io {
    device: AudioObjectID,
    proc_id: AudioDeviceIOProcID,
    rt: *mut Rt,
}

impl Drop for Io {
    fn drop(&mut self) {
        // SAFETY: after the IO proc is destroyed no cycle can use `rt`.
        unsafe {
            AudioDeviceStop(self.device, self.proc_id);
            AudioDeviceDestroyIOProcID(self.device, self.proc_id);
            drop(Box::from_raw(self.rt));
        }
    }
}

impl Session {
    fn start(device: AudioObjectID, chain: &Chain, audible: bool) -> Result<Self, String> {
        let own = audio::own_process();
        if audible && own.is_none() {
            // Without this our own output would be captured again.
            return Err("macOS does not know this process yet".into());
        }
        let uid = audio::uid(device).ok_or("The output device has no identifier")?;
        let tap = Tap::new(own, audible)?;
        let aggregate = Aggregate::new(&uid, &tap.uuid)?;
        let rate = audio::sample_rate(aggregate.id)
            .or_else(|| audio::sample_rate(device))
            .ok_or("The output device has no sample rate")?;
        let (send, incoming) = RingBuffer::new(4);
        let (give_back, retired) = RingBuffer::new(8);
        let shared = Arc::new(Shared::default());
        let rt = Box::into_raw(Rt::new(
            Box::new(Stereo::new(chain, rate)),
            incoming,
            give_back,
            audible,
            shared.clone(),
        ));
        let mut proc_id: AudioDeviceIOProcID = None;
        // SAFETY: `rt` stays valid until Io is dropped.
        let status = unsafe {
            AudioDeviceCreateIOProcID(aggregate.id, Some(io_proc), rt.cast(), NonNull::from(&mut proc_id))
        };
        if let Err(e) = check("Opening the output device", status) {
            // SAFETY: no IO proc was created, so nothing else holds `rt`.
            drop(unsafe { Box::from_raw(rt) });
            return Err(e);
        }
        let io = Io { device: aggregate.id, proc_id, rt };
        audio::use_only_tap(aggregate.id, proc_id);
        // SAFETY: the IO proc id belongs to this device.
        check("Starting the output device", unsafe { AudioDeviceStart(aggregate.id, proc_id) })?;
        Ok(Self {
            io,
            _aggregate: aggregate,
            _tap: tap,
            device,
            rate,
            audible,
            chain: chain.clone(),
            send,
            retired,
            pending: None,
            shared,
            cycles: (0, Instant::now()),
        })
    }

    fn update(&mut self, chain: Chain) {
        while self.retired.pop().is_ok() {}
        if chain == self.chain && self.pending.is_none() {
            return;
        }
        match self.send.push(Box::new(Stereo::new(&chain, self.rate))) {
            Ok(()) => {
                self.chain = chain;
                self.pending = None;
            }
            // The audio thread is still fading; deliver the newest soon.
            Err(_) => self.pending = Some(chain),
        }
    }

    /// False when the device changed under us or stopped calling back.
    fn healthy(&mut self) -> bool {
        let cycles = self.shared.cycles.load(Ordering::Relaxed);
        if cycles != self.cycles.0 {
            self.cycles = (cycles, Instant::now());
        }
        self.cycles.1.elapsed() < Duration::from_secs(4)
            && audio::sample_rate(self.io.device).is_none_or(|r| r == self.rate)
    }
}

fn load(dir: &Path) -> Option<State> {
    serde_json::from_slice(&std::fs::read(state_path(dir)).ok()?).ok()
}

struct Engine {
    dir: PathBuf,
    state: State,
    session: Option<Session>,
    /// Set once sound was captured, which proves permission was given.
    allowed: bool,
    failures: u32,
    retry_at: Option<Instant>,
    report: Option<Report>,
}

impl Engine {
    fn say(&mut self, phase: Phase, detail: &str) {
        let r = Report {
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").into(),
            phase,
            detail: detail.into(),
        };
        if self.report.as_ref() != Some(&r) {
            if let Ok(json) = serde_json::to_vec(&r) {
                let _ = swap_file(&report_path(&self.dir), &json);
            }
            self.report = Some(r);
        }
    }

    /// Brings the audio in line with the settings. Returns how long to wait
    /// before looking again.
    fn step(&mut self) -> Duration {
        let Some(device) = audio::default_output() else {
            self.session = None;
            self.say(Phase::Flat, "No output device");
            return Duration::from_secs(1);
        };
        let profile = audio::uid(device).and_then(|u| self.state.devices.get(&u));
        let rate = audio::sample_rate(device).unwrap_or(48_000.0);
        let chain = profile.map(|p| p.chain(rate)).unwrap_or_default();
        if chain.left.is_empty() && chain.right.is_empty() && chain.preamp_db == 0.0 {
            self.session = None;
            self.say(Phase::Flat, "");
            return Duration::from_secs(1);
        }
        if let Some(s) = &mut self.session {
            let heard = s.shared.heard.load(Ordering::Relaxed);
            if heard {
                self.allowed = true;
            }
            let wrong_mode = self.allowed && !s.audible;
            if s.device == device && !wrong_mode && s.healthy() {
                s.update(chain);
                let busy = s.pending.is_some();
                let phase = if s.audible { Phase::Active } else { Phase::Waiting };
                self.failures = 0;
                self.say(phase, "");
                return Duration::from_millis(if busy {
                    10
                } else if phase == Phase::Waiting {
                    150
                } else {
                    1000
                });
            }
            // Stop the old session completely before starting another.
            self.session = None;
        }
        if let Some(t) = self.retry_at
            && Instant::now() < t
        {
            return Duration::from_millis(250);
        }
        match Session::start(device, &chain, self.allowed) {
            Ok(s) => {
                self.retry_at = None;
                let phase = if s.audible { Phase::Active } else { Phase::Waiting };
                self.session = Some(s);
                self.say(phase, "");
                Duration::from_millis(150)
            }
            Err(e) => {
                self.failures += 1;
                // Back to listening, which needs less to succeed.
                if self.failures > 2 {
                    self.allowed = false;
                }
                let wait = Duration::from_millis(500 * u64::from(self.failures.min(20)));
                self.retry_at = Some(Instant::now() + wait);
                self.say(Phase::Failed, &e);
                Duration::from_millis(250)
            }
        }
    }
}

/// Runs until the equalizer is turned off. Returns at once when another engine
/// is already running.
pub fn run(dir: PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(&dir)?;
    let path = socket_path(&dir);
    if UnixDatagram::unbound()?.send_to(b"reload", &path).is_ok() {
        return Ok(());
    }
    let _ = std::fs::remove_file(&path);
    let socket = UnixDatagram::bind(&path)?;
    let mut engine = Engine {
        state: load(&dir).unwrap_or_default(),
        dir,
        session: None,
        allowed: false,
        failures: 0,
        retry_at: None,
        report: None,
    };
    let mut buf = [0u8; 64];
    while engine.state.enabled {
        let wait = engine.step();
        socket.set_read_timeout(Some(wait))?;
        match socket.recv(&mut buf) {
            Ok(n) if &buf[..n] == b"quit" => break,
            Ok(n) if &buf[..n] == b"reload" => {
                if let Some(s) = load(&engine.dir) {
                    engine.state = s;
                }
            }
            _ => {}
        }
    }
    engine.session = None;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(report_path(&engine.dir));
    Ok(())
}
