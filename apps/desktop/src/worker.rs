//! Runs the audio backend away from the window, so a slow system call never
//! freezes the interface.

use eq_backend::{Action, Backend, Check, Device, Level, State, Status};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

pub enum Command {
    Apply(State),
    Checks,
    Act(Action),
    StartAtLogin(bool),
    Quit,
}

#[derive(Clone, Debug)]
pub enum Event {
    Devices(Vec<Device>),
    Status(Status),
    Checks(Vec<Check>),
    Message(Level, String),
    LoginSupported(bool),
}

/// Sends commands to wherever the backend lives.
pub trait Engine {
    fn send(&mut self, command: Command);
}

pub type Emit = Box<dyn Fn(Event) + Send>;

struct Threaded {
    tx: Sender<Command>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Engine for Threaded {
    fn send(&mut self, command: Command) {
        let quit = matches!(command, Command::Quit);
        let _ = self.tx.send(command);
        if quit && let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

const POLL: Duration = Duration::from_millis(1500);

pub fn spawn(make: impl FnOnce() -> Box<dyn Backend> + Send + 'static, emit: Emit) -> Box<dyn Engine> {
    let (tx, rx) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("engine".into())
        .spawn(move || run(make(), &rx, &emit))
        .ok();
    Box::new(Threaded { tx, thread })
}

struct Loop<'a> {
    backend: Box<dyn Backend>,
    emit: &'a Emit,
    state: Option<State>,
    devices: Vec<Device>,
    status: Option<Status>,
    last_error: Option<String>,
}

impl Loop<'_> {
    fn apply(&mut self) {
        let Some(state) = &self.state else { return };
        let result = self.backend.apply(state);
        let error = result.err();
        if error != self.last_error {
            if let Some(e) = &error {
                (self.emit)(Event::Message(Level::Bad, e.clone()));
            }
            self.last_error = error;
        }
    }

    fn poll(&mut self) {
        let devices = self.backend.devices();
        if devices != self.devices {
            self.devices = devices;
            (self.emit)(Event::Devices(self.devices.clone()));
        }
        let status = self.backend.status();
        if self.status.as_ref() != Some(&status) {
            (self.emit)(Event::Status(status.clone()));
            self.status = Some(status);
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Apply(s) => {
                self.state = Some(s);
                self.apply();
            }
            Command::Checks => {
                let checks = self.backend.checks();
                (self.emit)(Event::Checks(checks));
            }
            Command::Act(a) => {
                match self.backend.act(a) {
                    Ok(m) => (self.emit)(Event::Message(Level::Good, m)),
                    Err(m) => (self.emit)(Event::Message(Level::Bad, m)),
                }
                self.last_error = None;
                self.apply();
                let checks = self.backend.checks();
                (self.emit)(Event::Checks(checks));
            }
            Command::StartAtLogin(on) => {
                if let Err(e) = self.backend.set_start_at_login(on) {
                    (self.emit)(Event::Message(Level::Bad, e));
                }
            }
            Command::Quit => {}
        }
    }
}

fn run(backend: Box<dyn Backend>, rx: &Receiver<Command>, emit: &Emit) {
    emit(Event::LoginSupported(backend.supports_start_at_login()));
    let mut l = Loop { backend, emit, state: None, devices: Vec::new(), status: None, last_error: None };
    l.poll();
    let mut next_poll = Instant::now() + POLL;
    loop {
        match rx.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(Command::Apply(mut newest)) => {
                // While dragging, only the latest settings matter.
                let mut after = Vec::new();
                while let Ok(c) = rx.try_recv() {
                    match c {
                        Command::Apply(s) if after.is_empty() => newest = s,
                        other => after.push(other),
                    }
                }
                l.handle(Command::Apply(newest));
                for c in after {
                    if matches!(c, Command::Quit) {
                        return;
                    }
                    l.handle(c);
                }
            }
            Ok(c) => l.handle(c),
            Err(RecvTimeoutError::Timeout) => {}
        }
        if Instant::now() >= next_poll {
            // Also re-applies, which repairs an engine that went away.
            l.apply();
            l.poll();
            next_poll = Instant::now() + POLL;
        }
    }
}

/// Answers every command at once on the calling thread. For demos and tests.
pub struct Immediate {
    backend: Box<dyn Backend>,
    emit: Emit,
}

impl Immediate {
    pub fn new(backend: Box<dyn Backend>, emit: Emit) -> Self {
        Self { backend, emit }
    }

    pub fn poll(&mut self) {
        (self.emit)(Event::Devices(self.backend.devices()));
        (self.emit)(Event::Status(self.backend.status()));
    }
}

impl Engine for Immediate {
    fn send(&mut self, command: Command) {
        match command {
            Command::Apply(s) => {
                let _ = self.backend.apply(&s);
            }
            Command::Checks => (self.emit)(Event::Checks(self.backend.checks())),
            Command::Act(a) => {
                let _ = self.backend.act(a);
            }
            Command::StartAtLogin(_) | Command::Quit => {}
        }
    }
}
