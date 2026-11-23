//! The app's state and everything the interface can do.

use crate::ui::*;
use crate::worker::{Command, Emit, Engine, Event};
use crate::{about, graph, presets, tone};
use eq_ai::{Config, Context, Http, Provider};
use eq_backend::{Check, Device, Level, State};
use eq_core::change::{self, Change};
use eq_core::filter::{Channel, Filter, Kind};
use eq_core::hearing::{self, Test, Threshold};
use eq_core::history::History;
use eq_core::profile::{Chain, Correction, HearingLayer, Preamp, Profile};
use eq_core::{autoeq, preset, rules, sliders, text, warning};
use eq_store::{AiChoice, DeviceState, Devices, Settings, Store, Theme, keys};
use slint::{ComponentHandle, Model as _, ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub const SHOW_FILE: &str = "show.request";
const RATE: f64 = 48_000.0;
/// Rows handed to the headphone list at once. Typing narrows it further.
const MAX_RESULTS: usize = 400;

thread_local! {
    static CORE: RefCell<Option<Core>> = const { RefCell::new(None) };
}

/// Runs `f` with the app state. Calls that arrive while the state is already
/// in use are put off until the event loop is free.
fn with(f: impl FnOnce(&mut Core) + 'static) {
    CORE.with(|c| match c.try_borrow_mut() {
        Ok(mut guard) => {
            if let Some(core) = guard.as_mut() {
                f(core);
            }
        }
        Err(_) => Timer::single_shot(Duration::ZERO, move || with(f)),
    });
}

struct Chat {
    from_user: bool,
    text: String,
    tone: Tone,
    changes: Vec<Change>,
    lines: Vec<String>,
    state: i32,
    origin: String,
    /// The bands before and after applying, to notice undo and redo.
    before: Vec<Filter>,
    after: Vec<Filter>,
}

enum NameFor {
    SaveAs,
    Rename(String),
}

struct Lists {
    bands: Rc<VecModel<Band>>,
    sliders: Rc<VecModel<Slider10>>,
    warnings: Rc<VecModel<Warning>>,
    presets: Rc<VecModel<PresetRow>>,
    headphones: Rc<VecModel<HeadphoneRow>>,
    chat: Rc<VecModel<ChatRow>>,
    devices: Rc<VecModel<DeviceRow>>,
    health: Rc<VecModel<HealthRow>>,
    bars: Rc<VecModel<EarBar>>,
}

struct Core {
    ui: Weak<AppWindow>,
    store: Store,
    engine: Box<dyn Engine>,
    settings: Settings,
    devices: Devices,
    outputs: Vec<Device>,
    device_id: Option<String>,
    profile: Profile,
    preset: Option<String>,
    presets: Vec<Profile>,
    builtin: usize,
    history: History,
    plot: graph::Plot,
    range: f64,
    range_grown: Instant,
    dragging: bool,
    comparing: bool,
    selected: i32,
    results: Vec<usize>,
    checks: Vec<Check>,
    chat: Vec<Chat>,
    chat_job: u64,
    models: Vec<String>,
    test: Option<Test>,
    thresholds: Option<Vec<Threshold>>,
    player: tone::Player,
    name_for: NameFor,
    lists: Lists,
    save_timer: Timer,
    toast_timer: Timer,
    tone_timer: Timer,
    frame_timer: Timer,
    frame_pending: bool,
    dirty_devices: bool,
    dirty_settings: bool,
}

/// Updates a model in place. Rows keep their identity, so a handle that is
/// being dragged is never rebuilt under the pointer.
fn sync<T: Clone + PartialEq + 'static>(model: &VecModel<T>, new: Vec<T>) {
    while model.row_count() > new.len() {
        model.remove(model.row_count() - 1);
    }
    for (i, v) in new.into_iter().enumerate() {
        if i >= model.row_count() {
            model.push(v);
        } else if model.row_data(i).as_ref() != Some(&v) {
            model.set_row_data(i, v);
        }
    }
}

fn strings(v: impl IntoIterator<Item = impl Into<SharedString>>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(v.into_iter().map(Into::into).collect::<Vec<_>>()))
}

fn level_tone(l: Level) -> Tone {
    match l {
        Level::Good => Tone::Good,
        Level::Info => Tone::Neutral,
        Level::Warning => Tone::Warning,
        Level::Bad => Tone::Danger,
    }
}

fn index(i: i32) -> Option<usize> {
    usize::try_from(i).ok()
}

/// Starts the app on an existing window.
pub fn start(ui: &AppWindow, store: Store, make_engine: impl FnOnce(Emit) -> Box<dyn Engine>) {
    let emit: Emit = Box::new(|event| {
        let _ = slint::invoke_from_event_loop(move || with(move |c| c.on_event(event)));
    });
    start_with(ui, store, make_engine(emit));
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(700), || with(|c| c.check_show_request()));
    KEEP.with(|k| k.borrow_mut().push(timer));
}

thread_local! {
    static KEEP: RefCell<Vec<Timer>> = const { RefCell::new(Vec::new()) };
}

/// Like [`start`], with an engine that answers on this thread.
pub fn start_with(ui: &AppWindow, store: Store, engine: Box<dyn Engine>) {
    let settings = store.settings();
    let devices = store.devices();
    let lists = Lists {
        bands: Rc::default(),
        sliders: Rc::default(),
        warnings: Rc::default(),
        presets: Rc::default(),
        headphones: Rc::default(),
        chat: Rc::default(),
        devices: Rc::default(),
        health: Rc::default(),
        bars: Rc::default(),
    };
    let m = ui.global::<Model>();
    m.set_bands(ModelRc::from(lists.bands.clone()));
    m.set_sliders(ModelRc::from(lists.sliders.clone()));
    m.set_warnings(ModelRc::from(lists.warnings.clone()));
    m.set_presets(ModelRc::from(lists.presets.clone()));
    m.set_headphone_results(ModelRc::from(lists.headphones.clone()));
    m.set_chat(ModelRc::from(lists.chat.clone()));
    m.set_devices(ModelRc::from(lists.devices.clone()));
    m.set_health(ModelRc::from(lists.health.clone()));
    m.set_hearing_bars(ModelRc::from(lists.bars.clone()));
    m.set_version(env!("CARGO_PKG_VERSION").into());
    m.set_platform(
        if cfg!(windows) {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Linux"
        }
        .into(),
    );
    m.set_grid_x(ModelRc::new(VecModel::from(graph::grid_x())));
    m.set_credits(ModelRc::new(VecModel::from(about::credits())));
    m.set_device_name("No output device".into());
    m.set_engine_status("Starting".into());
    m.set_engine_tone(Tone::Busy);
    m.set_slider_range(12.0);
    m.set_slider_view(settings.slider_view);
    connect(ui);
    let pixels = include_bytes!("../assets/icon-64.rgba");
    let icon = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(pixels, 64, 64);
    ui.set_app_icon(slint::Image::from_rgba8(icon));

    // The graph may have reported its size before anyone listened.
    let (w, h) = (f64::from(m.get_plot_width()), f64::from(m.get_plot_height()));
    let plot = if w >= 8.0 && h >= 8.0 {
        graph::Plot::new(w, h, RATE)
    } else {
        graph::Plot::new(800.0, 300.0, RATE)
    };
    let builtin = presets::builtin();
    let mut core = Core {
        ui: ui.as_weak(),
        store,
        engine,
        settings,
        devices,
        outputs: Vec::new(),
        device_id: None,
        profile: Profile::default(),
        preset: None,
        builtin: builtin.len(),
        presets: builtin,
        history: History::default(),
        plot,
        range: 12.0,
        range_grown: Instant::now(),
        dragging: false,
        comparing: false,
        selected: -1,
        results: Vec::new(),
        checks: Vec::new(),
        chat: Vec::new(),
        chat_job: 0,
        models: Vec::new(),
        test: None,
        thresholds: None,
        player: tone::Player::default(),
        name_for: NameFor::SaveAs,
        lists,
        save_timer: Timer::default(),
        toast_timer: Timer::default(),
        tone_timer: Timer::default(),
        frame_timer: Timer::default(),
        frame_pending: false,
        dirty_devices: false,
        dirty_settings: false,
    };
    core.load_presets();
    core.refresh_settings();
    core.refresh_chat();
    core.search("");
    core.refresh();
    CORE.with(|c| *c.borrow_mut() = Some(core));
}

/// Passes an engine event to the app. Used by engines on this thread.
pub fn event(e: Event) {
    with(move |c| c.on_event(e));
}

/// Saves everything. The sound keeps running.
pub fn finish() {
    CORE.with(|c| {
        if let Some(mut core) = c.borrow_mut().take() {
            core.player.stop();
            if core.test.is_some() || core.comparing {
                core.test = None;
                core.comparing = false;
                core.send_sound();
            }
            core.save_now();
            core.engine.send(Command::Quit);
        }
    });
    KEEP.with(|k| k.borrow_mut().clear());
}

impl Core {
    fn set(&self, f: impl FnOnce(&Model<'_>)) {
        if let Some(ui) = self.ui.upgrade() {
            f(&ui.global::<Model>());
        }
    }

    // ---- sound ----

    fn chain(&self) -> Chain {
        self.profile.chain(RATE)
    }

    /// Changes the profile as one undoable step.
    fn edit(&mut self, key: Option<&str>, f: impl FnOnce(&mut Profile)) {
        let mut next = self.profile.clone();
        f(&mut next);
        if next == self.profile {
            return;
        }
        self.history.record(&self.profile, key);
        self.profile = next;
        if self.comparing {
            self.comparing = false;
        }
        self.refresh_soon();
    }

    /// Drag events can arrive faster than the screen updates. One redraw per
    /// frame is enough.
    fn refresh_soon(&mut self) {
        if !self.dragging {
            self.refresh();
            return;
        }
        if self.frame_pending {
            return;
        }
        self.frame_pending = true;
        self.frame_timer.start(TimerMode::SingleShot, Duration::from_millis(12), || {
            with(|c| {
                if c.frame_pending {
                    c.refresh();
                }
            });
        });
    }

    fn send_sound(&mut self) {
        let mut state = State { enabled: self.settings.enabled, ..State::default() };
        for (id, d) in &self.devices.devices {
            state.devices.insert(id.clone(), d.profile.clone());
        }
        if let Some(id) = &self.device_id {
            let mut p = self.profile.clone();
            if self.comparing {
                // Same preamp, no filters: a fair comparison at one volume.
                let preamp = self.chain().preamp_db;
                let db = if preamp == 0.0 { -0.01 } else { preamp };
                p = Profile { preamp: Preamp { auto: false, db }, ..Profile::default() };
            }
            state.devices.insert(id.clone(), p);
        }
        if self.test.is_some() {
            // The hearing test must not be equalized.
            state.devices.clear();
        }
        self.engine.send(Command::Apply(state));
    }

    /// Redraws everything that depends on the sound, and makes it audible.
    fn refresh(&mut self) {
        self.frame_pending = false;
        let chain = self.chain();
        let warnings = warning::check(&self.profile, RATE);
        let left = self.plot.grid.response(&chain.left);
        let right = if chain.is_symmetric() { None } else { Some(self.plot.grid.response(&chain.right)) };
        if !self.dragging {
            let top = left
                .iter()
                .chain(right.iter().flatten())
                .copied()
                .chain(self.profile.filters.iter().filter(|f| f.kind.has_gain()).map(|f| f.gain_db))
                // High and low pass filters fall without end; they should not zoom out the graph.
                .filter(|v| *v > -60.0)
                .fold(0.0f64, |a, b| a.max(if b > 0.0 { b } else { -b * 0.999 }));
            let steep = self.profile.filters.iter().any(|f| f.on && !f.kind.has_gain());
            let want = if steep {
                self.profile
                    .filters
                    .iter()
                    .filter(|f| f.kind.has_gain())
                    .map(|f| f.gain_db.abs())
                    .fold(0.0, f64::max)
            } else {
                top
            };
            self.range = graph::range_for(want);
        }
        let range = self.range;
        let total = self.plot.path(&left, range);
        let fill = self.plot.fill(&total, range);
        let right_path = right.map(|r| self.plot.path(&r, range)).unwrap_or_default();
        let (cl, _) = self.profile.correction_chain();
        let correction =
            if cl.is_empty() { String::new() } else { self.plot.path(&self.plot.grid.response(&cl), range) };
        let selected_path = index(self.selected)
            .and_then(|i| self.profile.filters.get(i))
            .filter(|f| f.is_active())
            .map(|f| {
                let plain = Filter { channel: Channel::Both, ..*f };
                self.plot.path(&self.plot.grid.response(&[plain]), range)
            })
            .unwrap_or_default();
        sync(&self.lists.bands, graph::bands(&self.profile, &warnings, range));
        let slider_rows: Vec<Slider10> = (0..sliders::FREQS.len())
            .map(|i| Slider10 {
                label: sliders::LABELS[i].into(),
                gain: sliders::position(&self.profile.filters, i) as f32,
            })
            .collect();
        let slider_top = slider_rows.iter().map(|s| f64::from(s.gain.abs())).fold(0.0, f64::max);
        sync(&self.lists.sliders, slider_rows);
        sync(
            &self.lists.warnings,
            warnings
                .iter()
                .map(|w| Warning {
                    tone: match w.severity {
                        warning::Severity::Info => Tone::Neutral,
                        warning::Severity::Caution => Tone::Warning,
                        warning::Severity::Danger => Tone::Danger,
                    },
                    title: w.title.as_str().into(),
                    detail: w.detail.as_str().into(),
                })
                .collect(),
        );
        let net = chain.net_peak_db();
        let dirty = self.is_dirty();
        let name = self.preset.clone().unwrap_or_else(|| "Custom".into());
        let dragging = self.dragging;
        let p = &self.profile;
        self.set(|m| {
            m.set_curve_total(total.into());
            m.set_curve_total_fill(fill.into());
            m.set_curve_right(right_path.into());
            m.set_curve_correction(correction.into());
            m.set_curve_selected(selected_path.into());
            m.set_grid_y(ModelRc::new(VecModel::from(graph::grid_y(range))));
            if !dragging {
                m.set_slider_range(if slider_top > 12.0 { graph::range_for(slider_top) as f32 } else { 12.0 });
            }
            m.set_preamp(chain.preamp_db as f32);
            m.set_auto_preamp(p.preamp.auto);
            m.set_peak_text(format!("Peak {}", text::gain(net)).into());
            m.set_clipping(net > 0.05);
            m.set_enabled(self.settings.enabled);
            m.set_comparing(self.comparing);
            m.set_can_undo(self.history.can_undo());
            m.set_can_redo(self.history.can_redo());
            m.set_profile_name(name.into());
            m.set_profile_dirty(dirty);
            m.set_selected_band(self.selected);
            m.set_headphone_active(p.headphone.as_ref().map(|h| h.name.as_str()).unwrap_or_default().into());
            m.set_headphone_on(p.headphone.as_ref().is_some_and(|h| h.on));
            m.set_headphone_strength(p.headphone.as_ref().map_or(1.0, |h| h.strength) as f32);
            m.set_hearing_on(p.hearing.as_ref().is_some_and(|h| h.on));
            m.set_hearing_has_result(p.hearing.is_some() || self.thresholds.is_some());
            m.set_hearing_strength(p.hearing.as_ref().map_or(0.5, |h| h.strength) as f32);
        });
        if !self.dragging {
            self.refresh_presets();
            self.refresh_bars();
            self.notice_chat_undo();
            self.refresh_privacy_text();
        }
        self.remember_device();
        if !self.dragging {
            self.refresh_devices();
        }
        self.send_sound();
    }

    fn current_preset(&self) -> Option<&Profile> {
        let name = self.preset.as_ref()?;
        self.presets.iter().find(|p| p.name == *name)
    }

    fn is_dirty(&self) -> bool {
        match self.current_preset() {
            Some(p) => p.filters != self.profile.filters || p.preamp != self.profile.preamp,
            None => !self.profile.filters.is_empty() || !self.profile.preamp.auto,
        }
    }

    // ---- devices ----

    fn remember_device(&mut self) {
        let Some(id) = self.device_id.clone() else { return };
        let name = self
            .outputs
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.name.clone())
            .or_else(|| self.devices.devices.get(&id).map(|d| d.name.clone()))
            .unwrap_or_default();
        let state = DeviceState { name, profile: self.profile.clone(), preset: self.preset.clone() };
        if self.devices.devices.get(&id) != Some(&state) {
            self.devices.devices.insert(id, state);
            self.dirty_devices = true;
            self.save_soon();
        }
    }

    fn on_devices(&mut self, outputs: Vec<Device>) {
        let default = outputs.iter().find(|d| d.is_default).cloned();
        self.outputs = outputs;
        let new_id = default.as_ref().map(|d| d.id.clone());
        if new_id != self.device_id {
            let first = self.device_id.is_none();
            self.seal();
            self.device_id = new_id.clone();
            let saved = new_id.as_ref().and_then(|id| self.devices.devices.get(id)).cloned();
            // A new device starts flat; what another device needs may not suit it.
            let (profile, preset) = match saved {
                Some(s) => (s.profile, s.preset),
                None => (Profile::default(), None),
            };
            self.profile = profile;
            self.preset = preset;
            self.history.clear();
            self.thresholds = None;
            self.selected = -1;
            self.comparing = false;
            self.test = None;
            self.player.stop();
            self.set(|m| m.set_hearing_step(0));
            self.refresh();
            if !first && let Some(d) = &default {
                let message = format!("Switched to {}", d.name);
                self.toast(Tone::Neutral, &message, false);
            }
        }
        let name = default.map_or_else(|| "No output device".to_owned(), |d| d.name);
        self.set(|m| m.set_device_name(name.into()));
        self.refresh_devices();
    }

    fn refresh_devices(&self) {
        let mut rows: Vec<DeviceRow> = self
            .outputs
            .iter()
            .map(|d| DeviceRow {
                name: d.name.as_str().into(),
                detail: if d.is_default { "Playing here".into() } else { "Connected".into() },
                profile: self.profile_label(&d.id).into(),
                is_default: d.is_default,
            })
            .collect();
        for (id, d) in &self.devices.devices {
            if !self.outputs.iter().any(|o| o.id == *id) {
                rows.push(DeviceRow {
                    name: d.name.as_str().into(),
                    detail: "Not connected".into(),
                    profile: self.profile_label(id).into(),
                    is_default: false,
                });
            }
        }
        sync(&self.lists.devices, rows);
    }

    fn profile_label(&self, id: &str) -> String {
        match self.devices.devices.get(id) {
            Some(d) if d.profile.filters.is_empty() && d.profile.headphone.is_none() && d.profile.hearing.is_none() => {
                "Flat".into()
            }
            Some(d) => d.preset.clone().unwrap_or_else(|| "Custom".into()),
            None => "Flat".into(),
        }
    }

    fn on_event(&mut self, e: Event) {
        match e {
            Event::Devices(d) => self.on_devices(d),
            Event::Status(s) => {
                self.set(|m| {
                    m.set_engine_status(s.text.into());
                    m.set_engine_tone(level_tone(s.level));
                });
            }
            Event::Checks(c) => {
                sync(
                    &self.lists.health,
                    c.iter()
                        .map(|c| HealthRow {
                            tone: level_tone(c.level),
                            title: c.title.as_str().into(),
                            detail: c.detail.as_str().into(),
                            action: c.action.as_ref().map(|a| a.0.as_str()).unwrap_or_default().into(),
                        })
                        .collect(),
                );
                self.checks = c;
                self.set(|m| m.set_health_busy(false));
            }
            Event::Message(level, text) => {
                self.store.log(&text);
                self.toast(level_tone(level), &text, false);
            }
            Event::LoginSupported(yes) => self.set(|m| m.set_start_at_login_supported(yes)),
        }
    }

    fn check_show_request(&mut self) {
        if std::fs::remove_file(self.store.path().join(SHOW_FILE)).is_ok()
            && let Some(ui) = self.ui.upgrade()
        {
            ui.window().set_minimized(false);
            let _ = ui.show();
        }
    }

    // ---- saving ----

    fn save_soon(&mut self) {
        // Not restarted while running, so constant edits still get saved.
        if !self.save_timer.running() {
            self.save_timer.start(TimerMode::SingleShot, Duration::from_millis(800), || with(Core::save_now));
        }
    }

    fn save_now(&mut self) {
        let mut failed = None;
        if std::mem::take(&mut self.dirty_devices) {
            self.devices.schema = 1;
            failed = self.store.save_devices(&self.devices).err();
        }
        if std::mem::take(&mut self.dirty_settings) {
            failed = self.store.save_settings(&self.settings).err().or(failed);
        }
        if let Some(e) = failed {
            self.store.log(&format!("save failed: {e}"));
            self.toast(Tone::Danger, &format!("Your settings could not be saved: {e}"), false);
        }
    }

    fn settings_changed(&mut self) {
        self.dirty_settings = true;
        self.save_soon();
        self.refresh_settings();
    }

    fn toast(&mut self, tone: Tone, text: &str, can_undo: bool) {
        self.set(|m| {
            m.set_toast(text.into());
            m.set_toast_tone(tone);
            m.set_toast_can_undo(can_undo);
        });
        let time = Duration::from_millis(2500 + 45 * text.len().min(120) as u64);
        self.toast_timer.start(TimerMode::SingleShot, time, || with(|c| c.set(|m| m.set_toast("".into()))));
    }

    /// Ends a continuous edit, so the next one is its own undo step.
    fn seal(&mut self) {
        self.history.seal();
        self.dragging = false;
    }

    fn after_jump(&mut self) {
        if index(self.selected).is_some_and(|i| i >= self.profile.filters.len()) {
            self.selected = -1;
        }
        self.comparing = false;
        self.refresh();
    }

    // ---- bands ----

    fn band_edit(&mut self, i: i32, key: Option<&str>, f: impl FnOnce(&mut Filter)) {
        let Some(i) = index(i).filter(|i| *i < self.profile.filters.len()) else { return };
        self.edit(key, |p| {
            let mut next = p.filters[i];
            f(&mut next);
            if next.is_valid() {
                p.filters[i] = next;
            }
        });
    }

    fn band_add(&mut self, f: Filter) {
        self.seal();
        self.selected = self.profile.filters.len() as i32;
        self.edit(None, |p| p.filters.push(f));
    }

    fn band_drag(&mut self, i: i32, x: f32, y: f32) {
        let Some(n) = index(i).filter(|n| *n < self.profile.filters.len()) else { return };
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        self.dragging = true;
        self.selected = i;
        let has_gain = self.profile.filters[n].kind.has_gain();
        let mut gain = graph::db_of(f64::from(y), self.range);
        if has_gain && gain.abs() > self.range {
            // Pushing past the edge zooms out step by step.
            if self.range_grown.elapsed() > Duration::from_millis(350) {
                self.range = graph::next_range(self.range);
                self.range_grown = Instant::now();
            }
            gain = gain.clamp(-self.range, self.range);
        }
        let freq = graph::round_freq(graph::freq_of(f64::from(x).clamp(0.0, 1.0)));
        let key = format!("drag {i}");
        self.band_edit(i, Some(&key), |f| {
            f.freq_hz = freq;
            if has_gain {
                f.gain_db = (gain * 10.0).round() / 10.0;
            }
        });
    }

    fn band_text(&mut self, i: i32, typed: &str, f: impl FnOnce(&mut Filter, f64)) {
        match text::parse_number(typed) {
            Some(v) => {
                self.seal();
                self.band_edit(i, None, |b| f(b, v));
                // Shows the stored value again when the number was not usable.
                self.refresh();
            }
            None => {
                self.toast(Tone::Warning, "That is not a number", false);
                self.refresh();
            }
        }
    }

    fn band_nudge(&mut self, i: i32, field: i32, steps: f32) {
        let s = f64::from(steps);
        let key = format!("nudge {i} {field}");
        self.band_edit(i, Some(&key), |f| match field {
            0 => f.freq_hz = graph::round_freq(f.freq_hz * 2f64.powf(s / 24.0)),
            1 if f.kind.has_gain() => f.gain_db = ((f.gain_db + s * 0.5) * 10.0).round() / 10.0,
            2 => f.q = (f.q * 1.06f64.powf(s) * 1000.0).round().max(1.0) / 1000.0,
            _ => {}
        });
    }

    fn slider(&mut self, i: i32, gain: f32) {
        let Some(n) = index(i) else { return };
        self.dragging = true;
        let g = (f64::from(gain) * 10.0).round() / 10.0;
        let key = format!("slider {i}");
        self.edit(Some(&key), |p| sliders::set(&mut p.filters, n, g));
    }

    // ---- presets ----

    fn load_presets(&mut self) {
        self.presets = presets::builtin();
        self.builtin = self.presets.len();
        self.presets.extend(self.store.presets());
        self.refresh_presets();
    }

    fn refresh_presets(&self) {
        let rows = self
            .presets
            .iter()
            .enumerate()
            .map(|(i, p)| PresetRow {
                name: p.name.as_str().into(),
                detail: match p.filters.len() {
                    0 => "No changes".into(),
                    1 => "1 band".into(),
                    n => slint::format!("{n} bands"),
                },
                active: self.preset.as_ref() == Some(&p.name) || (self.preset.is_none() && i == 0 && !self.is_dirty()),
                builtin: i < self.builtin,
            })
            .collect();
        sync(&self.lists.presets, rows);
    }

    fn preset_load(&mut self, i: i32) {
        let Some(p) = index(i).and_then(|i| self.presets.get(i)).cloned() else { return };
        self.seal();
        self.selected = -1;
        self.preset = Some(p.name.clone());
        let same = p.filters == self.profile.filters && p.preamp == self.profile.preamp;
        if same {
            self.refresh();
            return;
        }
        let name = p.name.clone();
        self.edit(None, |x| {
            x.filters = p.filters;
            x.preamp = p.preamp;
            x.name = p.name;
        });
        self.toast(Tone::Neutral, &format!("Loaded {name}"), true);
    }

    fn save_preset_as(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if presets::builtin().iter().any(|p| p.name == name) {
            self.toast(Tone::Warning, "A built-in preset already has this name", false);
            return;
        }
        let p = Profile {
            name: name.into(),
            headphone: None,
            hearing: None,
            ..self.profile.clone()
        };
        match self.store.save_preset(&p) {
            Ok(()) => {
                self.preset = Some(name.into());
                self.profile.name = name.into();
                self.load_presets();
                self.refresh();
                self.set(|m| m.set_name_dialog(false));
                self.toast(Tone::Good, &format!("Saved {name}"), false);
            }
            Err(e) => self.toast(Tone::Danger, &format!("The preset could not be saved: {e}"), false),
        }
    }

    fn preset_save(&mut self) {
        let own = self.preset.clone().filter(|n| self.presets[self.builtin..].iter().any(|p| p.name == *n));
        match own {
            Some(name) => self.save_preset_as(&name),
            None => self.ask_name(NameFor::SaveAs, "Save preset", ""),
        }
    }

    fn ask_name(&mut self, purpose: NameFor, title: &str, value: &str) {
        self.name_for = purpose;
        self.set(|m| {
            m.set_name_dialog_title(title.into());
            m.set_name_dialog_text(value.into());
            m.set_name_dialog(true);
        });
    }

    fn name_accepted(&mut self, name: &str) {
        let name: String = name.trim().chars().take(80).collect();
        if name.is_empty() {
            return;
        }
        match std::mem::replace(&mut self.name_for, NameFor::SaveAs) {
            NameFor::SaveAs => self.save_preset_as(&name),
            NameFor::Rename(old) => {
                if old != name && self.presets.iter().any(|p| p.name == name) {
                    self.name_for = NameFor::Rename(old);
                    self.toast(Tone::Warning, "Another preset already has this name", false);
                    return;
                }
                if let Err(e) = self.store.rename_preset(&old, &name) {
                    self.toast(Tone::Danger, &format!("The preset could not be renamed: {e}"), false);
                }
                if self.preset.as_ref() == Some(&old) {
                    self.preset = Some(name.clone());
                }
                for d in self.devices.devices.values_mut() {
                    if d.preset.as_ref() == Some(&old) {
                        d.preset = Some(name.clone());
                        self.dirty_devices = true;
                    }
                }
                self.set(|m| m.set_name_dialog(false));
                self.load_presets();
                self.refresh();
            }
        }
    }

    fn preset_delete(&mut self, i: i32) {
        let Some(i) = index(i).filter(|i| *i >= self.builtin && *i < self.presets.len()) else { return };
        let name = self.presets[i].name.clone();
        match self.store.delete_preset(&name) {
            Ok(()) => {
                if self.preset.as_ref() == Some(&name) {
                    self.preset = None;
                }
                self.load_presets();
                self.refresh();
                self.toast(Tone::Neutral, &format!("Deleted {name}"), false);
            }
            Err(e) => self.toast(Tone::Danger, &format!("The preset could not be deleted: {e}"), false),
        }
    }

    /// Loads a preset file. Also used by tests.
    fn import_text(&mut self, name: &str, content: &str) {
        let (profile, skipped) = if content.trim_start().starts_with('{') {
            match Profile::from_json(content) {
                Ok(p) => (p, 0),
                Err(e) => return self.toast(Tone::Danger, &e, false),
            }
        } else {
            let parsed = preset::parse(content);
            let skipped = parsed.skipped.len();
            if parsed.filters.is_empty() && parsed.preamp_db.is_none() {
                return self.toast(Tone::Warning, "No equalizer settings were found in this file", false);
            }
            (parsed.into_profile(name), skipped)
        };
        self.seal();
        self.selected = -1;
        self.preset = None;
        self.edit(None, |p| {
            p.filters = profile.filters;
            p.preamp = profile.preamp;
        });
        let mut message = format!("Imported {name}");
        if skipped > 0 {
            message += &format!(". {skipped} lines are not supported and were left out");
        }
        self.toast(if skipped > 0 { Tone::Warning } else { Tone::Good }, &message, true);
    }

    // ---- headphones ----

    fn search(&mut self, query: &str) {
        self.results = autoeq::search(query);
        let active = self.profile.headphone.as_ref().map(|h| (h.name.clone(), h.source.clone()));
        let rows = self
            .results
            .iter()
            .take(MAX_RESULTS)
            .filter_map(|i| autoeq::get(*i))
            .map(|e| HeadphoneRow {
                name: e.name.into(),
                source: e.source.into(),
                form: e.form.into(),
                active: active.as_ref().is_some_and(|a| a.0 == e.name && a.1 == e.source),
            })
            .collect();
        sync(&self.lists.headphones, rows);
        let matches = self.results.len() as i32;
        self.set(|m| {
            m.set_headphone_total(autoeq::len() as i32);
            m.set_headphone_matches(matches);
        });
    }

    fn search_again(&mut self) {
        let q = self.ui.upgrade().map(|ui| ui.global::<Model>().get_headphone_query()).unwrap_or_default();
        self.search(&q);
    }

    fn headphone_pick(&mut self, i: i32) {
        let Some(e) = index(i).and_then(|i| self.results.get(i)).and_then(|i| autoeq::get(*i)) else { return };
        self.seal();
        self.edit(None, |p| {
            p.headphone = Some(Correction {
                name: e.name.into(),
                source: e.source.into(),
                on: true,
                strength: 1.0,
                filters: e.filters(),
            });
        });
        self.search_again();
        self.toast(Tone::Good, &format!("Corrected for {}", e.name), true);
    }

    // ---- hearing test ----

    fn hearing_start(&mut self) {
        self.seal();
        self.test = Some(Test::new());
        self.comparing = false;
        self.send_sound();
        self.set(|m| m.set_hearing_step(1));
        // Time for the engine to step aside before the first tone.
        self.tone_timer.start(TimerMode::SingleShot, Duration::from_millis(900), || with(Core::play_tone));
        self.refresh_test(true);
    }

    fn play_tone(&mut self) {
        let Some(tone) = self.test.as_ref().and_then(Test::tone) else { return };
        match self.player.play(tone) {
            Ok(length) => {
                self.refresh_test(true);
                self.tone_timer.start(TimerMode::SingleShot, length, || {
                    with(|c| {
                        c.player.stop();
                        c.refresh_test(false);
                    });
                });
            }
            Err(e) => {
                self.hearing_cancel();
                self.toast(Tone::Danger, &format!("The tone could not be played: {e}"), false);
            }
        }
    }

    fn refresh_test(&self, playing: bool) {
        let Some(t) = &self.test else { return };
        let Some(tone) = t.tone() else { return };
        self.set(|m| {
            m.set_hearing_ear(if tone.ear == hearing::Ear::Left { "Left ear" } else { "Right ear" }.into());
            m.set_hearing_tone(text::freq(tone.freq).into());
            m.set_hearing_progress(t.progress() as f32);
            m.set_hearing_playing(playing);
        });
    }

    fn hearing_answer(&mut self, heard: bool) {
        let Some(t) = &mut self.test else { return };
        t.answer(heard);
        self.player.stop();
        if t.is_done() {
            self.thresholds = Some(t.thresholds());
            self.test = None;
            self.set(|m| {
                m.set_hearing_playing(false);
                m.set_hearing_progress(1.0);
                m.set_hearing_step(2);
            });
            self.refresh();
            return;
        }
        self.refresh_test(false);
        self.tone_timer.start(TimerMode::SingleShot, Duration::from_millis(450), || with(Core::play_tone));
    }

    fn hearing_cancel(&mut self) {
        self.player.stop();
        self.tone_timer.stop();
        self.test = None;
        self.set(|m| {
            m.set_hearing_playing(false);
            m.set_hearing_step(0);
        });
        self.send_sound();
    }

    fn hearing_apply(&mut self) {
        let Some(t) = &self.thresholds else { return };
        let comp = hearing::compensation(t);
        let left: Vec<(f64, f64)> = comp.iter().map(|c| (c.0, c.1)).collect();
        let right: Vec<(f64, f64)> = comp.iter().map(|c| (c.0, c.2)).collect();
        let layer = HearingLayer {
            on: true,
            strength: self.profile.hearing.as_ref().map_or(0.5, |h| h.strength),
            left: hearing::fit(&left, RATE),
            right: hearing::fit(&right, RATE),
        };
        let nothing = layer.left.is_empty() && layer.right.is_empty();
        self.seal();
        self.edit(None, |p| p.hearing = Some(layer));
        if nothing {
            self.toast(Tone::Good, "Both ears hear evenly, so nothing needs to change", true);
        } else {
            self.toast(Tone::Good, "Hearing compensation is on", true);
        }
    }

    fn refresh_bars(&self) {
        let rows: Vec<EarBar> = if let Some(t) = &self.thresholds {
            hearing::compensation(t)
                .iter()
                .zip(t)
                .map(|(c, t)| EarBar {
                    label: text::freq(c.0).into(),
                    left: c.1 as f32,
                    right: c.2 as f32,
                    tested: t.left.is_some() && t.right.is_some(),
                })
                .collect()
        } else if let Some(h) = &self.profile.hearing {
            let mut freqs = hearing::FREQS;
            freqs.sort_by(f64::total_cmp);
            freqs
                .iter()
                .map(|f| EarBar {
                    label: text::freq(*f).into(),
                    left: eq_core::curve::response_at(&h.left, *f, RATE) as f32,
                    right: eq_core::curve::response_at(&h.right, *f, RATE) as f32,
                    tested: true,
                })
                .collect()
        } else {
            Vec::new()
        };
        sync(&self.lists.bars, rows);
    }

    // ---- assistant ----

    fn ai_config(&self) -> Option<Config> {
        let provider = match self.settings.ai {
            AiChoice::Rules => return None,
            AiChoice::Anthropic => Provider::Anthropic,
            AiChoice::OpenAi => Provider::OpenAi,
            AiChoice::Ollama => Provider::Ollama,
        };
        let id = self.settings.ai.id();
        let s = self.settings.ai_settings.get(id).cloned().unwrap_or_default();
        Some(Config { provider, url: s.url, model: s.model, key: keys::get(id) })
    }

    fn device_label(&self) -> String {
        self.outputs.iter().find(|d| d.is_default).map_or_else(|| "Unknown".into(), |d| d.name.clone())
    }

    fn ai_context(&self) -> Context {
        Context::new(&self.profile, &self.device_label(), self.settings.send_hearing)
    }

    fn refresh_privacy_text(&self) {
        let sends = match self.settings.ai {
            AiChoice::Rules => "Nothing. The built-in rules run on this computer.".to_owned(),
            _ => format!("Your message, your last few messages in this chat, and:\n{}", self.ai_context().summary()),
        };
        self.set(|m| m.set_assistant_sends(sends.trim_end().into()));
    }

    fn assistant_send(&mut self, words: &str) {
        let words: String = words.trim().chars().take(2000).collect();
        if words.is_empty() || self.chat.last().is_some_and(|c| c.tone == Tone::Busy) {
            return;
        }
        self.push_chat(true, &words, Tone::Neutral, "");
        let Some(config) = self.ai_config() else {
            self.answer_with_rules(&words, None);
            return;
        };
        let mut ctx = self.ai_context();
        let mut question: Option<&str> = None;
        for c in &self.chat[..self.chat.len() - 1] {
            if c.from_user {
                question = Some(&c.text);
            } else if c.tone != Tone::Danger
                && let Some(q) = question.take()
            {
                ctx.earlier.push((q.to_owned(), c.text.clone()));
            }
        }
        let origin = config.model.clone();
        self.push_chat(false, "Thinking", Tone::Busy, &origin);
        self.chat_job += 1;
        let job = self.chat_job;
        let filters = self.profile.filters.clone();
        std::thread::spawn(move || {
            let http = Http::new(Duration::from_secs(60));
            let result = eq_ai::ask(&http, &config, &ctx, &words);
            let _ = slint::invoke_from_event_loop(move || {
                with(move |c| {
                    if c.chat_job != job {
                        return;
                    }
                    c.chat.pop();
                    match result {
                        Ok(a) => c.propose(&a.reason, a.changes, &origin, &filters),
                        Err(e) => {
                            let e = e.to_string();
                            c.store.log(&format!("assistant: {e}"));
                            c.answer_with_rules(&words, Some(&e));
                        }
                    }
                });
            });
        });
    }

    fn answer_with_rules(&mut self, words: &str, problem: Option<&str>) {
        if let Some(p) = problem {
            self.push_chat(false, p, Tone::Danger, "");
        }
        match rules::suggest(words, &self.profile.filters) {
            Some(s) => {
                let filters = self.profile.filters.clone();
                self.propose(&s.reason, s.changes, "Built-in rules", &filters);
            }
            None if problem.is_none() => self.push_chat(
                false,
                "I did not understand that. Try words like bass, vocals, harsh, muddy, warm or bright.",
                Tone::Neutral,
                "Built-in rules",
            ),
            None => {}
        }
    }

    fn propose(&mut self, reason: &str, changes: Vec<Change>, origin: &str, filters: &[Filter]) {
        let mut text = if reason.is_empty() { "Here is a suggestion.".to_owned() } else { reason.to_owned() };
        let mut preview = Profile { filters: filters.to_vec(), ..self.profile.clone() };
        let usable = change::apply_changes(&mut preview, &changes).is_ok();
        if !usable {
            return self.push_chat(false, "The suggestion could not be used.", Tone::Danger, origin);
        }
        let old = warning::check(&self.profile, RATE);
        for w in warning::check(&preview, RATE) {
            if w.severity > warning::Severity::Info && !old.iter().any(|o| o.title == w.title) {
                text += &format!("\nNote: {}. {}", w.title, w.detail);
            }
        }
        let lines = changes.iter().map(|c| change::describe_change(filters, c)).collect();
        self.chat.push(Chat {
            from_user: false,
            text,
            tone: Tone::Neutral,
            state: i32::from(!changes.is_empty()),
            changes,
            lines,
            origin: origin.into(),
            before: filters.to_vec(),
            after: preview.filters,
        });
        self.refresh_chat();
    }

    fn push_chat(&mut self, from_user: bool, text: &str, tone: Tone, origin: &str) {
        self.chat.push(Chat {
            from_user,
            text: text.into(),
            tone,
            changes: Vec::new(),
            lines: Vec::new(),
            state: 0,
            origin: origin.into(),
            before: Vec::new(),
            after: Vec::new(),
        });
        self.refresh_chat();
    }

    fn assistant_apply(&mut self, i: i32) {
        let Some(i) = index(i).filter(|i| self.chat.get(*i).is_some_and(|c| matches!(c.state, 1 | 3 | 4))) else {
            return;
        };
        if self.chat[i].before != self.profile.filters {
            self.toast(Tone::Warning, "Your bands changed since this suggestion. Ask again for a new one", false);
            return;
        }
        let changes = self.chat[i].changes.clone();
        self.seal();
        self.selected = -1;
        self.edit(None, |p| {
            // Checked when the suggestion was made.
            let _ = change::apply_changes(p, &changes);
        });
        self.chat[i].state = 2;
        self.refresh_chat();
        self.toast(Tone::Good, "Applied", true);
    }

    fn notice_chat_undo(&mut self) {
        let mut changed = false;
        for c in &mut self.chat {
            let new = match c.state {
                2 if c.before == self.profile.filters => 4,
                4 if c.after == self.profile.filters => 2,
                s => s,
            };
            changed |= new != c.state;
            c.state = new;
        }
        if changed {
            self.refresh_chat();
        }
    }

    fn refresh_chat(&self) {
        let rows = self.chat.iter().map(|c| ChatRow {
            from_user: c.from_user,
            text: c.text.as_str().into(),
            tone: c.tone,
            changes: strings(c.lines.iter().map(String::as_str)),
            state: c.state,
            origin: c.origin.as_str().into(),
        });
        // Rows hold nested lists, which never compare equal; replace as needed.
        let rows: Vec<ChatRow> = rows.collect();
        let model = &self.lists.chat;
        while model.row_count() > rows.len() {
            model.remove(model.row_count() - 1);
        }
        for (i, r) in rows.into_iter().enumerate() {
            match model.row_data(i) {
                None => model.push(r),
                Some(old) if old.text != r.text || old.state != r.state || old.tone != r.tone => {
                    model.set_row_data(i, r);
                }
                Some(_) => {}
            }
        }
        let busy = self.chat.last().is_some_and(|c| c.tone == Tone::Busy);
        self.set(|m| m.set_assistant_busy(busy));
    }

    // ---- settings ----

    fn refresh_settings(&self) {
        let s = &self.settings;
        let id = s.ai.id();
        let ai = s.ai_settings.get(id).cloned().unwrap_or_default();
        let provider = AiChoice::ALL.iter().position(|c| *c == s.ai).unwrap_or(0) as i32;
        let default_url = match s.ai {
            AiChoice::Rules => "",
            AiChoice::Anthropic => Provider::Anthropic.default_url(),
            AiChoice::OpenAi => Provider::OpenAi.default_url(),
            AiChoice::Ollama => Provider::Ollama.default_url(),
        };
        let has_key = matches!(s.ai, AiChoice::Anthropic | AiChoice::OpenAi) && keys::get(id).is_some();
        self.set(|m| {
            m.set_theme_mode(match s.theme {
                Theme::System => 0,
                Theme::Dark => 1,
                Theme::Light => 2,
            });
            m.set_provider(provider);
            m.set_provider_url(if ai.url.is_empty() { default_url } else { &ai.url }.into());
            m.set_provider_needs_url(matches!(s.ai, AiChoice::OpenAi | AiChoice::Ollama));
            m.set_provider_needs_key(matches!(s.ai, AiChoice::Anthropic | AiChoice::OpenAi));
            m.set_provider_has_key(has_key);
            m.set_provider_key_hint(slint::format!("Kept in the {}", keys::store_name()));
            m.set_provider_model(ai.model.as_str().into());
            m.set_provider_models(strings(self.models.iter().map(String::as_str)));
            m.set_send_hearing(s.send_hearing);
            m.set_start_at_login(s.start_at_login);
            m.set_assistant_provider(
                match s.ai {
                    AiChoice::Rules => "Built-in rules".to_owned(),
                    _ if ai.model.is_empty() => "No model chosen".to_owned(),
                    _ => ai.model.clone(),
                }
                .into(),
            );
        });
        self.refresh_privacy_text();
    }

    fn provider_status(&self, tone: Tone, text: &str) {
        self.set(|m| {
            m.set_provider_status(text.into());
            m.set_provider_status_tone(tone);
        });
    }

    fn set_provider(&mut self, i: i32) {
        let Some(c) = index(i).and_then(|i| AiChoice::ALL.get(i)) else { return };
        self.settings.ai = *c;
        self.models.clear();
        self.provider_status(Tone::Neutral, "");
        self.settings_changed();
    }

    fn ai_settings(&mut self) -> &mut eq_store::AiSettings {
        self.settings.ai_settings.entry(self.settings.ai.id().into()).or_default()
    }

    fn load_models(&mut self) {
        let Some(config) = self.ai_config() else { return };
        self.provider_status(Tone::Busy, "Loading models");
        let choice = self.settings.ai;
        std::thread::spawn(move || {
            let result = eq_ai::models(&Http::new(Duration::from_secs(20)), &config);
            let _ = slint::invoke_from_event_loop(move || {
                with(move |c| {
                    if c.settings.ai != choice {
                        return;
                    }
                    match result {
                        Ok(models) if models.is_empty() => {
                            c.provider_status(Tone::Warning, "The provider lists no models. Type a model name instead.");
                        }
                        Ok(models) => {
                            let text = format!("Connected. {} models available.", models.len());
                            if c.ai_settings().model.is_empty() {
                                c.ai_settings().model = models[0].clone();
                            }
                            c.models = models;
                            c.settings_changed();
                            c.provider_status(Tone::Good, &text);
                        }
                        Err(e) => c.provider_status(Tone::Danger, &e.to_string()),
                    }
                });
            });
        });
    }

    fn diagnostics(&self) -> String {
        let mut s = format!(
            "EQaura {} on {} {}\n",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        s += &format!("Enabled: {}\nOutput devices: {}\n", self.settings.enabled, self.outputs.len());
        s += &format!("Bands: {}\nAssistant: {}\n\nChecks\n", self.profile.filters.len(), self.settings.ai.id());
        for c in &self.checks {
            s += &format!("[{:?}] {}: {}\n", c.level, c.title, c.detail);
        }
        s += "\nLog\n";
        let log = self.store.log_text();
        let tail: Vec<&str> = log.lines().rev().take(200).collect();
        for l in tail.iter().rev() {
            s += l;
            s.push('\n');
        }
        s
    }
}

/// Runs a file dialog outside the state borrow, so the window stays alive.
fn pick_file(f: impl FnOnce() -> Option<std::path::PathBuf> + 'static, then: impl FnOnce(&mut Core, std::path::PathBuf) + 'static) {
    Timer::single_shot(Duration::ZERO, move || {
        if let Some(path) = f() {
            with(move |c| then(c, path));
        }
    });
}

fn connect(ui: &AppWindow) {
    let m = ui.global::<Model>();
    // equalizer
    m.on_graph_resized(|w, h| {
        with(move |c| {
            let (w, h) = (f64::from(w), f64::from(h));
            if w >= 8.0 && h >= 8.0 && ((w - c.plot.w).abs() > 0.5 || (h - c.plot.h).abs() > 0.5) {
                c.plot = graph::Plot::new(w, h, RATE);
                c.refresh();
            }
        });
    });
    m.on_set_enabled(|on| {
        with(move |c| {
            c.settings.enabled = on;
            c.comparing = false;
            c.settings_changed();
            c.refresh();
        });
    });
    m.on_set_comparing(|on| {
        with(move |c| {
            c.comparing = on && c.settings.enabled;
            c.refresh();
        });
    });
    m.on_undo(|| {
        with(|c| {
            if c.history.undo(&mut c.profile) {
                c.after_jump();
            }
        });
    });
    m.on_redo(|| {
        with(|c| {
            if c.history.redo(&mut c.profile) {
                c.after_jump();
            }
        });
    });
    m.on_reset(|| {
        with(|c| {
            c.seal();
            c.selected = -1;
            c.preset = None;
            c.edit(None, |p| {
                p.filters.clear();
                p.preamp = Preamp::default();
            });
            c.toast(Tone::Neutral, "All bands removed", true);
        });
    });
    m.on_band_add(|x, y| {
        with(move |c| {
            let freq = graph::round_freq(graph::freq_of(f64::from(x).clamp(0.0, 1.0)));
            let gain = (graph::db_of(f64::from(y).clamp(0.0, 1.0), c.range) * 10.0).round() / 10.0;
            c.band_add(Filter::peak(freq, gain, 1.0));
        });
    });
    m.on_band_add_default(|| {
        with(|c| {
            // Fills the largest gap between existing bands.
            let mut xs: Vec<f64> =
                c.profile.filters.iter().map(|f| graph::x_of(f.freq_hz).clamp(0.0, 1.0)).collect();
            xs.extend([0.0, 1.0]);
            xs.sort_by(f64::total_cmp);
            let (a, b) = xs
                .windows(2)
                .map(|w| (w[0], w[1]))
                .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
                .unwrap_or((0.0, 1.0));
            let freq = graph::round_freq(graph::freq_of((a + b) / 2.0));
            c.band_add(Filter::peak(freq, 0.0, 1.0));
        });
    });
    m.on_band_remove(|i| {
        with(move |c| {
            let Some(n) = index(i).filter(|n| *n < c.profile.filters.len()) else { return };
            c.seal();
            let left = c.profile.filters.len() as i32 - 1;
            c.selected = if c.selected == i {
                i.min(left - 1)
            } else if c.selected > i {
                c.selected - 1
            } else {
                c.selected
            };
            c.edit(None, |p| {
                p.filters.remove(n);
            });
        });
    });
    m.on_band_drag(|i, x, y| with(move |c| c.band_drag(i, x, y)));
    m.on_band_drag_end(|_| {
        with(|c| {
            c.seal();
            c.refresh();
        });
    });
    m.on_band_width(|i, steps| {
        with(move |c| {
            let key = format!("width {i}");
            c.band_edit(i, Some(&key), |f| {
                f.q = (f.q * 1.08f64.powf(f64::from(steps)) * 1000.0).round().max(1.0) / 1000.0;
            });
        });
    });
    m.on_band_set_on(|i, on| {
        with(move |c| {
            c.seal();
            c.band_edit(i, None, |f| f.on = on);
        });
    });
    m.on_band_set_kind(|i, k| {
        with(move |c| {
            if let Some(kind) = index(k).and_then(|k| Kind::ALL.get(k)) {
                c.seal();
                c.band_edit(i, None, |f| f.kind = *kind);
            }
        });
    });
    m.on_band_set_channel(|i, k| {
        with(move |c| {
            if let Some(ch) = index(k).and_then(|k| Channel::ALL.get(k)) {
                c.seal();
                c.band_edit(i, None, |f| f.channel = *ch);
            }
        });
    });
    m.on_band_set_freq(|i, s| with(move |c| c.band_text(i, &s, |f, v| f.freq_hz = v)));
    m.on_band_set_gain(|i, s| with(move |c| c.band_text(i, &s, |f, v| f.gain_db = v)));
    m.on_band_set_q(|i, s| with(move |c| c.band_text(i, &s, |f, v| f.q = v)));
    m.on_band_nudge(|i, field, steps| with(move |c| c.band_nudge(i, field, steps)));
    m.on_slider_changed(|i, g| with(move |c| c.slider(i, g)));
    m.on_slider_released(|_| {
        with(|c| {
            c.seal();
            sliders::tidy(&mut c.profile.filters);
            c.after_jump();
        });
    });
    m.on_set_auto_preamp(|on| {
        with(move |c| {
            c.seal();
            let now = c.chain().preamp_db;
            c.edit(None, |p| p.preamp = Preamp { auto: on, db: now });
        });
    });
    m.on_set_preamp(|s| {
        with(move |c| {
            match text::parse_number(&s) {
                Some(db) => {
                    c.seal();
                    c.edit(None, |p| p.preamp = Preamp { auto: false, db });
                }
                None => c.toast(Tone::Warning, "That is not a number", false),
            }
            c.refresh();
        });
    });
    m.on_nudge_preamp(|delta| {
        with(move |c| {
            let db = ((c.chain().preamp_db + f64::from(delta)) * 10.0).round() / 10.0;
            c.edit(Some("preamp"), |p| p.preamp = Preamp { auto: false, db });
        });
    });
    // presets
    m.on_preset_load(|i| with(move |c| c.preset_load(i)));
    m.on_preset_delete(|i| with(move |c| c.preset_delete(i)));
    m.on_preset_rename(|i| {
        with(move |c| {
            if let Some(i) = index(i).filter(|i| *i >= c.builtin && *i < c.presets.len()) {
                let name = c.presets[i].name.clone();
                c.ask_name(NameFor::Rename(name.clone()), "Rename preset", &name);
            }
        });
    });
    m.on_preset_save(|| with(Core::preset_save));
    m.on_preset_save_as(|| {
        with(|c| {
            let name = c.preset.clone().filter(|n| !presets::builtin().iter().any(|p| p.name == *n));
            c.ask_name(NameFor::SaveAs, "Save preset", &name.unwrap_or_default());
        });
    });
    m.on_name_dialog_accept(|s| with(move |c| c.name_accepted(&s)));
    m.on_name_dialog_cancel(|| with(|c| c.set(|m| m.set_name_dialog(false))));
    m.on_preset_import(|| {
        pick_file(
            || {
                rfd::FileDialog::new()
                    .set_title("Import a preset")
                    .add_filter("Equalizer presets", &["txt", "json"])
                    .pick_file()
            },
            |c, path| {
                let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                // Presets are small; anything larger is the wrong file.
                let ok = std::fs::metadata(&path).is_ok_and(|m| m.len() < 4_000_000);
                match std::fs::read(&path) {
                    Ok(bytes) if ok => c.import_text(&name, &String::from_utf8_lossy(&bytes)),
                    Ok(_) => c.toast(Tone::Warning, "This file is too large to be a preset", false),
                    Err(e) => c.toast(Tone::Danger, &format!("The file could not be read: {e}"), false),
                }
            },
        );
    });
    m.on_preset_export(|| {
        pick_file(
            || {
                rfd::FileDialog::new()
                    .set_title("Export as Equalizer APO text")
                    .set_file_name("EQaura preset.txt")
                    .add_filter("Text", &["txt"])
                    .save_file()
            },
            |c, path| {
                let mut p = c.profile.clone();
                p.name = c.preset.clone().unwrap_or_else(|| "EQaura preset".into());
                match std::fs::write(&path, preset::export(&p)) {
                    Ok(()) => c.toast(Tone::Good, "Preset exported", false),
                    Err(e) => c.toast(Tone::Danger, &format!("The file could not be written: {e}"), false),
                }
            },
        );
    });
    // headphones
    m.on_headphone_search(|q| with(move |c| c.search(&q)));
    m.on_headphone_pick(|i| with(move |c| c.headphone_pick(i)));
    m.on_headphone_clear(|| {
        with(|c| {
            c.seal();
            c.edit(None, |p| p.headphone = None);
            c.search_again();
        });
    });
    m.on_headphone_set_on(|on| {
        with(move |c| {
            c.seal();
            c.edit(None, |p| {
                if let Some(h) = &mut p.headphone {
                    h.on = on;
                }
            });
        });
    });
    m.on_headphone_set_strength(|v| {
        with(move |c| {
            let v = f64::from(v.clamp(0.0, 1.0));
            c.edit(Some("headphone strength"), |p| {
                if let Some(h) = &mut p.headphone {
                    h.strength = v;
                }
            });
        });
    });
    // hearing test
    m.on_hearing_start(|| with(Core::hearing_start));
    m.on_hearing_answer(|heard| with(move |c| c.hearing_answer(heard)));
    m.on_hearing_replay(|| with(Core::play_tone));
    m.on_hearing_cancel(|| with(Core::hearing_cancel));
    m.on_hearing_apply(|| with(Core::hearing_apply));
    m.on_hearing_set_on(|on| {
        with(move |c| {
            c.seal();
            c.edit(None, |p| {
                if let Some(h) = &mut p.hearing {
                    h.on = on;
                }
            });
        });
    });
    m.on_hearing_set_strength(|v| {
        with(move |c| {
            let v = f64::from(v.clamp(0.0, 1.0));
            c.edit(Some("hearing strength"), |p| {
                if let Some(h) = &mut p.hearing {
                    h.strength = v;
                }
            });
        });
    });
    m.on_hearing_clear(|| {
        with(|c| {
            c.seal();
            c.thresholds = None;
            c.set(|m| m.set_hearing_step(0));
            c.edit(None, |p| p.hearing = None);
            c.refresh();
        });
    });
    // assistant
    m.on_assistant_send(|s| with(move |c| c.assistant_send(&s)));
    m.on_assistant_cancel(|| {
        with(|c| {
            c.chat_job += 1;
            if c.chat.last().is_some_and(|x| x.tone == Tone::Busy) {
                c.chat.pop();
                c.push_chat(false, "Stopped.", Tone::Neutral, "");
            }
        });
    });
    m.on_assistant_apply(|i| with(move |c| c.assistant_apply(i)));
    m.on_assistant_dismiss(|i| {
        with(move |c| {
            if let Some(x) = index(i).and_then(|i| c.chat.get_mut(i)).filter(|x| x.state == 1) {
                x.state = 3;
                c.refresh_chat();
            }
        });
    });
    m.on_assistant_clear(|| {
        with(|c| {
            c.chat_job += 1;
            c.chat.clear();
            c.refresh_chat();
        });
    });
    // system
    m.on_health_refresh(|| {
        with(|c| {
            c.set(|m| m.set_health_busy(true));
            c.engine.send(Command::Checks);
        });
    });
    m.on_health_action(|i| {
        with(move |c| {
            if let Some((_, a)) = index(i).and_then(|i| c.checks.get(i)).and_then(|x| x.action.clone()) {
                c.set(|m| m.set_health_busy(true));
                c.engine.send(Command::Act(a));
            }
        });
    });
    // settings
    m.on_set_theme_mode(|i| {
        with(move |c| {
            c.settings.theme = match i {
                1 => Theme::Dark,
                2 => Theme::Light,
                _ => Theme::System,
            };
            c.settings_changed();
        });
    });
    m.on_set_provider(|i| with(move |c| c.set_provider(i)));
    m.on_set_provider_url(|s| {
        with(move |c| {
            c.ai_settings().url = s.trim().into();
            c.settings_changed();
        });
    });
    m.on_set_provider_key(|s| {
        with(move |c| {
            let key = s.trim();
            if key.is_empty() {
                return;
            }
            match keys::set(c.settings.ai.id(), key) {
                keys::Saved::System => c.provider_status(Tone::Good, "Key saved."),
                keys::Saved::SessionOnly => c.provider_status(
                    Tone::Warning,
                    "No system key store is available, so the key is kept only until EQaura closes.",
                ),
            }
            c.refresh_settings();
        });
    });
    m.on_clear_provider_key(|| {
        with(|c| {
            keys::clear(c.settings.ai.id());
            c.provider_status(Tone::Neutral, "Key deleted.");
            c.refresh_settings();
        });
    });
    m.on_set_provider_model(|s| {
        with(move |c| {
            c.ai_settings().model = s.trim().into();
            c.settings_changed();
        });
    });
    m.on_provider_load_models(|| with(Core::load_models));
    m.on_set_send_hearing(|on| {
        with(move |c| {
            c.settings.send_hearing = on;
            c.settings_changed();
        });
    });
    m.on_set_start_at_login(|on| {
        with(move |c| {
            c.settings.start_at_login = on;
            c.engine.send(Command::StartAtLogin(on));
            c.settings_changed();
        });
    });
    m.on_open_data_folder(|| {
        with(|c| {
            if let Err(e) = eq_backend::open_path(&c.store.path().to_string_lossy()) {
                c.toast(Tone::Danger, &e, false);
            }
        });
    });
    m.on_export_diagnostics(|| {
        pick_file(
            || rfd::FileDialog::new().set_title("Save diagnostics").set_file_name("EQaura diagnostics.txt").save_file(),
            |c, path| match std::fs::write(&path, c.diagnostics()) {
                Ok(()) => c.toast(Tone::Good, "Diagnostics saved", false),
                Err(e) => c.toast(Tone::Danger, &format!("The file could not be written: {e}"), false),
            },
        );
    });
    m.on_open_link(|s| {
        let link = match s.as_str() {
            "" | "github" | "repo" => about::REPO.to_owned(),
            s if s.starts_with("https://") => s.to_owned(),
            _ => return,
        };
        let _ = eq_backend::open_path(&link);
    });
}
