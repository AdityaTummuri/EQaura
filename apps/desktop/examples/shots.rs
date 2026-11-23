//! Renders every page of the UI to PNG files without opening a window.
//!
//!   cargo run -q -p eqaura --example shots -- <outdir> [filters]
//!
//! Filters (all optional):
//!   --scene <text>     only scenes whose name contains the text
//!   --theme dark|light only one theme
//!   --size WxH         only one size (default: 1180x760 and 900x600)
//!   --scale N          only one scale factor (default: 1, plus 2 for the main pages)
//!
//! The sample data is realistic on purpose: curves come from real RBJ biquad maths.
slint::include_modules!();

use slint::Model as _;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint::{ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static CLOCK_MS: AtomicU64 = AtomicU64::new(0);

struct Headless(Rc<MinimalSoftwareWindow>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
    fn duration_since_start(&self) -> Duration {
        Duration::from_millis(CLOCK_MS.load(Ordering::Relaxed))
    }
}

// ---------------------------------------------------------------------------
// Equalizer maths (RBJ cookbook biquads)
// ---------------------------------------------------------------------------

const FS: f64 = 48_000.0;
const RANGE_DB: f64 = 15.0;

#[derive(Clone, Copy)]
struct Spec {
    on: bool,
    kind: i32, // 0 peak, 1 low shelf, 2 high shelf, 3 low pass, 4 high pass, 5 notch
    channel: i32,
    freq: f64,
    gain: f64,
    q: f64,
    warn: bool,
}

fn has_gain(kind: i32) -> bool {
    kind <= 2
}

fn response_db(s: &Spec, f: f64) -> f64 {
    let a = 10f64.powf(if has_gain(s.kind) { s.gain } else { 0.0 } / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * s.freq / FS;
    let (sin, cos) = w0.sin_cos();
    let alpha = sin / (2.0 * s.q);
    let sa = a.sqrt();
    let (b0, b1, b2, a0, a1, a2) = match s.kind {
        0 => (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a),
        1 => (
            a * ((a + 1.0) - (a - 1.0) * cos + 2.0 * sa * alpha),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - 2.0 * sa * alpha),
            (a + 1.0) + (a - 1.0) * cos + 2.0 * sa * alpha,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - 2.0 * sa * alpha,
        ),
        2 => (
            a * ((a + 1.0) + (a - 1.0) * cos + 2.0 * sa * alpha),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - 2.0 * sa * alpha),
            (a + 1.0) - (a - 1.0) * cos + 2.0 * sa * alpha,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - 2.0 * sa * alpha,
        ),
        3 => ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        4 => ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        _ => (1.0, -2.0 * cos, 1.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
    };
    let w = 2.0 * std::f64::consts::PI * f / FS;
    let (c1, s1) = (w.cos(), w.sin());
    let (c2, s2) = ((2.0 * w).cos(), (2.0 * w).sin());
    let nr = b0 + b1 * c1 + b2 * c2;
    let ni = -(b1 * s1 + b2 * s2);
    let dr = a0 + a1 * c1 + a2 * c2;
    let di = -(a1 * s1 + a2 * s2);
    10.0 * ((nr * nr + ni * ni) / (dr * dr + di * di)).log10()
}

fn x_pos(f: f64) -> f64 {
    (f / 20.0).ln() / (1000f64).ln()
}
fn y_pos(db: f64) -> f64 {
    0.5 - db / (2.0 * RANGE_DB)
}

fn curve_path(specs: &[&Spec], w: f64, h: f64, extra: &dyn Fn(f64) -> f64, close_to_bottom: bool) -> String {
    let n = (w / 3.0).max(60.0) as usize;
    let mut out = String::new();
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let f = 20.0 * 1000f64.powf(t);
        let db: f64 = specs.iter().map(|s| response_db(s, f)).sum::<f64>() + extra(f);
        let (x, y) = (t * w, y_pos(db) * h);
        out.push_str(&format!("{}{:.1} {:.1} ", if i == 0 { "M" } else { "L" }, x, y));
    }
    if close_to_bottom {
        out.push_str(&format!("L{:.1} {:.1} L0 {:.1} Z", w, h, h));
    }
    out
}

struct Eq {
    specs: Vec<Spec>,
    selected: i32,
    plot: (f64, f64),
    split_ears: bool,
    correction: bool,
}

fn correction_db(f: f64) -> f64 {
    // a gentle headphone correction: bass lift, upper-mid dip, treble lift
    let g = |c: f64, w: f64, a: f64| a * (-((f / c).ln().powi(2)) / (2.0 * w * w)).exp();
    g(70.0, 0.6, 2.2) + g(3200.0, 0.35, -1.6) + g(9000.0, 0.4, 1.8)
}

fn push_eq(app: &AppWindow, eq: &Eq) {
    let m = app.global::<Model>();
    let (w, h) = eq.plot;
    let bands: Vec<Band> = eq
        .specs
        .iter()
        .map(|s| Band {
            on: s.on,
            kind: s.kind,
            channel: s.channel,
            freq: s.freq as f32,
            gain: s.gain as f32,
            q: s.q as f32,
            has_gain: has_gain(s.kind),
            x: x_pos(s.freq) as f32,
            y: y_pos(if has_gain(s.kind) { s.gain } else { 0.0 }) as f32,
            warn: s.warn,
        })
        .collect();
    m.set_bands(ModelRc::new(VecModel::from(bands)));
    m.set_selected_band(eq.selected);
    let active: Vec<&Spec> = eq.specs.iter().filter(|s| s.on).collect();
    let left: Vec<&Spec> = active.iter().copied().filter(|s| s.channel != 2).collect();
    let right: Vec<&Spec> = active.iter().copied().filter(|s| s.channel != 1).collect();
    let corr = |f: f64| if eq.correction { correction_db(f) } else { 0.0 };
    m.set_curve_total(curve_path(&left, w, h, &corr, false).into());
    m.set_curve_total_fill(curve_path(&left, w, h, &corr, true).into());
    m.set_curve_right(
        if eq.split_ears { curve_path(&right, w, h, &corr, false) } else { String::new() }.into(),
    );
    m.set_curve_correction(
        if eq.correction { curve_path(&[], w, h, &correction_db, false) } else { String::new() }.into(),
    );
    m.set_curve_selected(
        eq.specs
            .get(eq.selected.max(0) as usize)
            .filter(|_| eq.selected >= 0)
            .map(|s| curve_path(&[s], w, h, &|_| 0.0, false))
            .unwrap_or_default()
            .into(),
    );
}

fn grids(m: &Model) {
    let mut gx = Vec::new();
    for dec in [10.0f64, 100.0, 1000.0, 10000.0] {
        for k in 1..10 {
            let f = (dec * k as f64).round();
            if !(20.0..=20000.0).contains(&f) {
                continue;
            }
            let label = match f as i64 {
                20 => "20",
                50 => "50",
                100 => "100",
                200 => "200",
                500 => "500",
                1000 => "1k",
                2000 => "2k",
                5000 => "5k",
                10000 => "10k",
                20000 => "20k",
                _ => "",
            };
            let strong = matches!(f as i64, 100 | 1000 | 10000);
            gx.push(GridLine { pos: x_pos(f) as f32, label: label.into(), strong });
        }
    }
    gx.push(GridLine { pos: 1.0, label: "20k".into(), strong: false });
    gx.sort_by(|a, b| a.pos.partial_cmp(&b.pos).unwrap());
    gx.dedup_by(|a, b| (a.pos - b.pos).abs() < 1e-4);
    m.set_grid_x(ModelRc::new(VecModel::from(gx)));
    let mut gy = Vec::new();
    let mut db = -12;
    while db <= 12 {
        let label = if db % 6 == 0 {
            if db == 0 {
                "0".to_string()
            } else if db > 0 {
                format!("+{db}")
            } else {
                format!("\u{2212}{}", -db)
            }
        } else {
            String::new()
        };
        gy.push(GridLine { pos: y_pos(db as f64) as f32, label: label.into(), strong: db == 0 });
        db += 3;
    }
    m.set_grid_y(ModelRc::new(VecModel::from(gy)));
}

fn sample_specs() -> Vec<Spec> {
    let s = |on, kind, channel, freq, gain, q, warn| Spec { on, kind, channel, freq, gain, q, warn };
    vec![
        s(true, 1, 0, 80.0, 3.0, 0.7, false),
        s(true, 0, 0, 160.0, -1.5, 1.1, false),
        s(true, 0, 0, 400.0, -2.0, 1.4, false),
        s(true, 0, 0, 1000.0, 1.0, 1.0, false),
        s(true, 0, 0, 2800.0, 7.5, 1.8, true),
        s(true, 0, 1, 4200.0, -2.5, 3.0, false),
        s(false, 0, 0, 6300.0, 2.0, 2.0, false),
        s(true, 5, 2, 7800.0, 0.0, 8.0, false),
        s(true, 2, 0, 10000.0, 2.5, 0.7, false),
        s(true, 3, 0, 18000.0, 0.0, 0.7, false),
    ]
}

fn strs(v: &[&str]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(v.iter().map(|s| SharedString::from(*s)).collect::<Vec<_>>()))
}
fn vm<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

// ---------------------------------------------------------------------------
// Base data
// ---------------------------------------------------------------------------

fn fill_base(app: &AppWindow, eq: &Rc<RefCell<Eq>>) {
    let m = app.global::<Model>();
    m.set_version("0.1.0".into());
    m.set_platform("macOS".into());
    m.set_enabled(true);
    m.set_engine_tone(Tone::Good);
    m.set_engine_status("Active".into());
    m.set_device_name("MacBook Pro Speakers".into());
    m.set_profile_name("Warm and relaxed".into());
    m.set_profile_dirty(true);
    m.set_can_undo(true);
    m.set_can_redo(false);
    m.set_comparing(false);
    m.set_toast("".into());

    // equalizer
    m.set_slider_view(false);
    m.set_slider_range(12.0);
    m.set_preamp(-3.5);
    m.set_auto_preamp(true);
    m.set_peak_text("Peak \u{2212}0.2 dB".into());
    m.set_clipping(false);
    m.set_warnings(vm(vec![Warning {
        tone: Tone::Warning,
        title: "Strong boost near 2.8 kHz".into(),
        detail: "This can sound harsh on some tracks. Lower the gain if it tires your ears.".into(),
    }]));
    let sl = [
        ("31", 1.0),
        ("62", 2.5),
        ("125", 1.5),
        ("250", -0.5),
        ("500", -2.0),
        ("1k", 1.0),
        ("2k", 3.0),
        ("4k", -1.5),
        ("8k", 2.0),
        ("16k", 1.0),
    ];
    m.set_sliders(vm(sl.iter().map(|(l, g)| Slider10 { label: (*l).into(), gain: *g as f32 }).collect()));
    grids(&m);
    {
        let mut e = eq.borrow_mut();
        e.specs = sample_specs();
        e.selected = 4;
        e.split_ears = true;
        e.correction = true;
        push_eq(app, &e);
    }

    // presets
    m.set_presets(vm(vec![
        PresetRow {
            name: "Warm and relaxed".into(),
            detail: "10 bands, saved today at 09:14".into(),
            active: true,
            builtin: false,
        },
        PresetRow {
            name: "Flat".into(),
            detail: "No changes to the sound".into(),
            active: false,
            builtin: true,
        },
        PresetRow {
            name: "Bass boost".into(),
            detail: "3 bands, built in".into(),
            active: false,
            builtin: true,
        },
        PresetRow {
            name: "Clearer vocals".into(),
            detail: "4 bands, built in".into(),
            active: false,
            builtin: true,
        },
        PresetRow {
            name: "Late night".into(),
            detail: "5 bands, saved yesterday".into(),
            active: false,
            builtin: false,
        },
        PresetRow {
            name: "Podcasts and calls".into(),
            detail: "6 bands, saved 3 days ago".into(),
            active: false,
            builtin: false,
        },
        PresetRow {
            name: "Studio check with a very long name that has to be shortened somewhere".into(),
            detail: "12 bands, saved last week".into(),
            active: false,
            builtin: false,
        },
    ]));
    m.set_name_dialog(false);
    m.set_name_dialog_title("Save preset".into());
    m.set_name_dialog_text("Warm and relaxed".into());

    // headphones
    let hp = |n: &str, s: &str, f: &str, a: bool| HeadphoneRow {
        name: n.into(),
        source: s.into(),
        form: f.into(),
        active: a,
    };
    m.set_headphone_query("".into());
    m.set_headphone_results(vm(vec![
        hp("Sennheiser HD 600", "oratory1990, measured on a GRAS 45CA", "Over-ear", true),
        hp("Sennheiser HD 650", "oratory1990, measured on a GRAS 45CA", "Over-ear", false),
        hp("Sennheiser HD 560S", "Crinacle, measured on a GRAS 43AG", "Over-ear", false),
        hp("Sennheiser HD 25", "Rtings, measured on a B&K 5128", "On-ear", false),
        hp("Sennheiser IE 200", "oratory1990, measured on a GRAS RA0045", "In-ear", false),
        hp("Sony WH-1000XM5", "Rtings, measured on a B&K 5128", "Over-ear", false),
        hp("Sony MDR-7506", "Headphone.com archive", "Over-ear", false),
        hp("Beyerdynamic DT 770 Pro 80 ohm", "oratory1990, measured on a GRAS 45CA", "Over-ear", false),
        hp("Beyerdynamic DT 1990 Pro", "Crinacle, measured on a GRAS 43AG", "Over-ear", false),
        hp("Audio-Technica ATH-M50x", "oratory1990, measured on a GRAS 45CA", "Over-ear", false),
        hp("Moondrop Aria", "Crinacle, measured on a GRAS RA0045", "In-ear", false),
        hp("Apple AirPods Pro 2", "Rtings, measured on a B&K 5128", "In-ear", false),
        hp("HiFiMAN Sundara", "oratory1990, measured on a GRAS 45CA", "Over-ear", false),
        hp("Focal Clear", "Crinacle, measured on a GRAS 43AG", "Over-ear", false),
    ]));
    m.set_headphone_total(4812);
    m.set_headphone_matches(4812);
    m.set_headphone_active("Sennheiser HD 600".into());
    m.set_headphone_on(true);
    m.set_headphone_strength(0.8);

    // hearing
    m.set_hearing_step(0);
    m.set_hearing_ear("Left ear".into());
    m.set_hearing_tone("2 kHz".into());
    m.set_hearing_progress(0.42);
    m.set_hearing_playing(true);
    let eb = |l: &str, a: f32, b: f32, t: bool| EarBar { label: l.into(), left: a, right: b, tested: t };
    m.set_hearing_bars(vm(vec![
        eb("125 Hz", 0.0, 0.0, false),
        eb("250 Hz", 1.0, 1.5, true),
        eb("500 Hz", 0.5, 1.0, true),
        eb("1 kHz", 1.5, 0.5, true),
        eb("2 kHz", 3.0, 2.0, true),
        eb("4 kHz", 5.5, 4.0, true),
        eb("8 kHz", 8.0, 6.5, true),
        eb("12 kHz", 9.5, 7.0, true),
    ]));
    m.set_hearing_on(true);
    m.set_hearing_has_result(true);
    m.set_hearing_strength(0.6);

    // assistant
    m.set_chat(vm(vec![
        ChatRow { from_user: true, text: "Vocals sound a bit harsh on my headphones, and I would like a little more bass.".into(), tone: Tone::Neutral, changes: ModelRc::default(), state: 0, origin: "".into() },
        ChatRow {
            from_user: false,
            text: "That usually comes from the upper mids around 3 kHz. I would soften that area a little and add a gentle lift below 100 Hz. Nothing is changed until you apply it.".into(),
            tone: Tone::Neutral,
            changes: strs(&["Lower 2.8 kHz by 2.5 dB (band 5)", "Add a low shelf at 90 Hz, +2.5 dB", "Raise the preamp cut to \u{2212}4.0 dB to keep headroom"]),
            state: 2,
            origin: "Anthropic, claude-sonnet".into(),
        },
        ChatRow { from_user: true, text: "Nice. Can it sound warmer overall?".into(), tone: Tone::Neutral, changes: ModelRc::default(), state: 0, origin: "".into() },
        ChatRow {
            from_user: false,
            text: "Yes. A small dip around 6 kHz and a soft rise near 250 Hz will take the edge off without dulling the sound.".into(),
            tone: Tone::Neutral,
            changes: strs(&["Add a peak at 250 Hz, +1.5 dB, wide", "Add a peak at 6.3 kHz, \u{2212}1.5 dB, medium width"]),
            state: 1,
            origin: "Anthropic, claude-sonnet".into(),
        },
    ]));
    m.set_assistant_busy(false);
    m.set_assistant_provider("Anthropic, claude-sonnet".into());
    m.set_assistant_sends("Your request text, the current band list (type, frequency, gain, width), the selected headphone model name and the preamp value. Your hearing test results are not sent. No audio ever leaves this computer.".into());

    // system
    m.set_devices(vm(vec![
        DeviceRow {
            name: "MacBook Pro Speakers".into(),
            detail: "48 kHz, 2 channels, built in".into(),
            profile: "Warm and relaxed".into(),
            is_default: true,
        },
        DeviceRow {
            name: "Sennheiser HD 600 on FiiO K3".into(),
            detail: "96 kHz, 2 channels, USB".into(),
            profile: "Flat".into(),
            is_default: false,
        },
        DeviceRow {
            name: "LG UltraFine Display Audio".into(),
            detail: "48 kHz, 2 channels, HDMI".into(),
            profile: "No profile".into(),
            is_default: false,
        },
    ]));
    m.set_health(vm(vec![
        HealthRow {
            tone: Tone::Good,
            title: "Audio driver found".into(),
            detail: "The virtual output device is installed and visible to apps.".into(),
            action: "".into(),
        },
        HealthRow {
            tone: Tone::Good,
            title: "Engine is running".into(),
            detail: "Processing 48 kHz audio with 2.1 ms of added delay.".into(),
            action: "".into(),
        },
        HealthRow {
            tone: Tone::Warning,
            title: "System output is not set to EQaura".into(),
            detail: "Apps are playing straight to MacBook Pro Speakers, so the equalizer is not applied."
                .into(),
            action: "Use EQaura output".into(),
        },
        HealthRow {
            tone: Tone::Danger,
            title: "Microphone permission is off".into(),
            detail: "Needed only to capture the system sound on this version of macOS.".into(),
            action: "Open settings".into(),
        },
    ]));
    m.set_health_busy(false);

    // settings
    m.set_theme_mode(1);
    m.set_provider(1);
    m.set_provider_url("https://api.anthropic.com".into());
    m.set_provider_needs_url(false);
    m.set_provider_needs_key(true);
    m.set_provider_has_key(true);
    m.set_provider_key_hint("Saved in your macOS Keychain".into());
    m.set_provider_models(strs(&["claude-sonnet", "claude-haiku", "claude-opus"]));
    m.set_provider_model("claude-sonnet".into());
    m.set_provider_status("Connected. 3 models found.".into());
    m.set_provider_status_tone(Tone::Good);
    m.set_send_hearing(false);
    m.set_start_at_login(true);
    m.set_start_at_login_supported(true);

    // about
    m.set_credits(vm(vec![
        Credit {
            name: "Slint".into(),
            license: "GPL-3.0 or royalty-free".into(),
            detail: "The user interface toolkit".into(),
        },
        Credit {
            name: "cpal".into(),
            license: "Apache-2.0".into(),
            detail: "Cross-platform audio input and output".into(),
        },
        Credit {
            name: "AutoEq measurements".into(),
            license: "MIT".into(),
            detail: "Headphone frequency response data and targets".into(),
        },
        Credit {
            name: "oratory1990".into(),
            license: "Measurements, with thanks".into(),
            detail: "Many of the headphone measurements used here".into(),
        },
        Credit {
            name: "serde".into(),
            license: "MIT or Apache-2.0".into(),
            detail: "Reading and writing presets".into(),
        },
    ]));
}

// ---------------------------------------------------------------------------
// Scenes
// ---------------------------------------------------------------------------

struct Scene {
    name: &'static str,
    page: Page,
    main: bool, // also rendered at 2x
    setup: fn(&AppWindow, &Rc<RefCell<Eq>>),
    tabs: u32,                 // Tab key presses before rendering, to show keyboard focus
    hover: Option<(f32, f32)>, // pointer position to show hover states
    click: Option<(f32, f32)>, // a click before rendering, to show popups
}

fn nothing(_: &AppWindow, _: &Rc<RefCell<Eq>>) {}

fn scenes() -> Vec<Scene> {
    vec![
        Scene {
            name: "eq",
            page: Page::Equalizer,
            main: true,
            tabs: 0,
            hover: None,
            click: None,
            setup: nothing,
        },
        Scene {
            name: "eq-sliders",
            page: Page::Equalizer,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| a.global::<Model>().set_slider_view(true),
        },
        Scene {
            name: "eq-off",
            page: Page::Equalizer,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| {
                let m = a.global::<Model>();
                m.set_enabled(false);
                m.set_engine_tone(Tone::Neutral);
                m.set_engine_status("Off".into());
            },
        },
        Scene {
            name: "eq-compare",
            page: Page::Equalizer,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| a.global::<Model>().set_comparing(true),
        },
        Scene {
            name: "eq-warnings",
            page: Page::Equalizer,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, e| {
                let m = a.global::<Model>();
                m.set_warnings(vm(vec![
                    Warning {
                        tone: Tone::Warning,
                        title: "Strong boost near 2.8 kHz".into(),
                        detail: "This can sound harsh on some tracks.".into(),
                    },
                    Warning {
                        tone: Tone::Warning,
                        title: "Total boost is high".into(),
                        detail: "The preamp lowers the level by 3.5 dB to keep the sound clean.".into(),
                    },
                    Warning {
                        tone: Tone::Danger,
                        title: "Peak is above 0 dB".into(),
                        detail: "Turn on Auto preamp or lower the preamp to avoid distortion.".into(),
                    },
                ]));
                m.set_clipping(true);
                m.set_peak_text("Peak +1.8 dB".into());
                m.set_auto_preamp(false);
                let _ = e;
            },
        },
        Scene {
            name: "eq-empty",
            page: Page::Equalizer,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, e| {
                let m = a.global::<Model>();
                {
                    let mut e = e.borrow_mut();
                    e.specs.clear();
                    e.selected = -1;
                    e.split_ears = false;
                    e.correction = false;
                    push_eq(a, &e);
                }
                m.set_warnings(ModelRc::default());
                m.set_sliders(vm((0..10)
                    .map(|i| Slider10 {
                        label: ["31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"][i].into(),
                        gain: 0.0,
                    })
                    .collect()));
                m.set_peak_text("Peak 0.0 dB".into());
            },
        },
        Scene {
            name: "eq-many",
            page: Page::Equalizer,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, e| {
                let mut e = e.borrow_mut();
                let mut v = Vec::new();
                for i in 0..28 {
                    let f = 20.0 * 1000f64.powf(i as f64 / 27.0);
                    v.push(Spec {
                        on: i % 9 != 5,
                        kind: 0,
                        channel: 0,
                        freq: f,
                        gain: ((i * 7) % 9) as f64 - 4.0,
                        q: 2.5,
                        warn: i == 12,
                    });
                }
                e.specs = v;
                e.selected = 13;
                e.split_ears = false;
                push_eq(a, &e);
            },
        },
        Scene {
            name: "eq-focus",
            page: Page::Equalizer,
            main: false,
            tabs: 4,
            hover: None,
            click: None,
            setup: nothing,
        },
        Scene {
            name: "eq-hover",
            page: Page::Equalizer,
            main: false,
            tabs: 0,
            hover: Some((1010.0, 94.0)),
            click: None,
            setup: nothing,
        },
        Scene {
            name: "hover-handle",
            page: Page::Equalizer,
            main: false,
            tabs: 0,
            hover: Some((650.0, 331.0)),
            click: None,
            setup: |a, e| {
                let mut e = e.borrow_mut();
                e.selected = 2;
                push_eq(a, &e);
            },
        },
        Scene {
            name: "settings-focus",
            page: Page::Settings,
            main: false,
            tabs: 12,
            hover: None,
            click: None,
            setup: nothing,
        },
        Scene {
            name: "eq-select",
            page: Page::Equalizer,
            main: false,
            tabs: 0,
            hover: None,
            click: Some((370.0, 720.0)),
            setup: nothing,
        },
        Scene {
            name: "eq-toast",
            page: Page::Equalizer,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| {
                let m = a.global::<Model>();
                m.set_toast("Removed band 6".into());
                m.set_toast_can_undo(true);
            },
        },
        Scene {
            name: "headphones",
            page: Page::Headphones,
            main: true,
            tabs: 0,
            hover: None,
            click: None,
            setup: nothing,
        },
        Scene {
            name: "headphones-many",
            page: Page::Headphones,
            main: false,
            tabs: 0,
            hover: Some((600.0, 480.0)),
            click: None,
            setup: |a, _| {
                let m = a.global::<Model>();
                m.set_headphone_total(5200);
                m.set_headphone_matches(5200);
                let rows: Vec<HeadphoneRow> = (0..5200)
                    .map(|i| HeadphoneRow {
                        name: format!("Model {i:04} Studio Monitor").into(),
                        source: "Sample source, measured on a GRAS 45CA".into(),
                        form: "Over-ear".into(),
                        active: i == 0,
                    })
                    .collect();
                m.set_headphone_results(vm(rows));
            },
        },
        Scene {
            name: "headphones-none",
            page: Page::Headphones,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| {
                let m = a.global::<Model>();
                m.set_headphone_active("".into());
            },
        },
        Scene {
            name: "headphones-nomatch",
            page: Page::Headphones,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| {
                let m = a.global::<Model>();
                m.set_headphone_active("".into());
                m.set_headphone_query("sennhauser hd9000".into());
                m.set_headphone_results(ModelRc::default());
                m.set_headphone_matches(0);
            },
        },
        Scene {
            name: "headphones-nodb",
            page: Page::Headphones,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| {
                let m = a.global::<Model>();
                m.set_headphone_active("".into());
                m.set_headphone_results(ModelRc::default());
                m.set_headphone_total(0);
                m.set_headphone_matches(0);
            },
        },
        Scene {
            name: "hearing-intro",
            page: Page::Hearing,
            main: true,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| a.global::<Model>().set_hearing_step(0),
        },
        Scene {
            name: "hearing-running",
            page: Page::Hearing,
            main: true,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| a.global::<Model>().set_hearing_step(1),
        },
        Scene {
            name: "hearing-result",
            page: Page::Hearing,
            main: true,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| a.global::<Model>().set_hearing_step(2),
        },
        Scene {
            name: "assistant",
            page: Page::Assistant,
            main: true,
            tabs: 0,
            hover: None,
            click: None,
            setup: nothing,
        },
        Scene {
            name: "assistant-busy",
            page: Page::Assistant,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| {
                let m = a.global::<Model>();
                m.set_assistant_busy(true);
                let mut rows: Vec<ChatRow> = m.get_chat().iter().collect();
                rows.push(ChatRow {
                    from_user: false,
                    text: "Thinking".into(),
                    tone: Tone::Busy,
                    changes: ModelRc::default(),
                    state: 0,
                    origin: "Anthropic, claude-sonnet".into(),
                });
                m.set_chat(vm(rows));
            },
        },
        Scene {
            name: "assistant-empty",
            page: Page::Assistant,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| {
                let m = a.global::<Model>();
                m.set_chat(ModelRc::default());
                m.set_assistant_provider("Built-in rules".into());
            },
        },
        Scene {
            name: "assistant-states",
            page: Page::Assistant,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| {
                let m = a.global::<Model>();
                let mut rows: Vec<ChatRow> = m.get_chat().iter().collect();
                rows[3].state = 3;
                rows.push(ChatRow {
                    from_user: true,
                    text: "Try again with less treble.".into(),
                    tone: Tone::Neutral,
                    changes: ModelRc::default(),
                    state: 0,
                    origin: "".into(),
                });
                rows.push(ChatRow {
                    from_user: false,
                    text: "I could not reach the provider. Check the connection in Settings and try again."
                        .into(),
                    tone: Tone::Danger,
                    changes: ModelRc::default(),
                    state: 0,
                    origin: "Anthropic, claude-sonnet".into(),
                });
                m.set_chat(vm(rows));
            },
        },
        Scene {
            name: "presets",
            page: Page::Presets,
            main: true,
            tabs: 0,
            hover: None,
            click: None,
            setup: nothing,
        },
        Scene {
            name: "presets-dialog",
            page: Page::Presets,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| a.global::<Model>().set_name_dialog(true),
        },
        Scene {
            name: "system",
            page: Page::System,
            main: true,
            tabs: 0,
            hover: None,
            click: None,
            setup: nothing,
        },
        Scene {
            name: "settings",
            page: Page::Settings,
            main: true,
            tabs: 0,
            hover: None,
            click: None,
            setup: nothing,
        },
        Scene {
            name: "settings-local",
            page: Page::Settings,
            main: false,
            tabs: 0,
            hover: None,
            click: None,
            setup: |a, _| {
                let m = a.global::<Model>();
                m.set_provider(3);
                m.set_provider_needs_url(true);
                m.set_provider_needs_key(false);
                m.set_provider_has_key(false);
                m.set_provider_url("http://localhost:11434".into());
                m.set_provider_models(ModelRc::default());
                m.set_provider_model("llama3.1".into());
                m.set_provider_status("Could not reach the server. Is Ollama running?".into());
                m.set_provider_status_tone(Tone::Danger);
                m.set_start_at_login_supported(false);
                m.set_theme_mode(0);
            },
        },
        Scene {
            name: "about",
            page: Page::About,
            main: true,
            tabs: 0,
            hover: None,
            click: None,
            setup: nothing,
        },
    ]
}

// ---------------------------------------------------------------------------
// Interaction self test: synthetic pointer and keyboard events against the real UI
// ---------------------------------------------------------------------------

#[derive(Default, Debug)]
struct Log {
    events: Vec<String>,
}

fn settle(win: &Rc<MinimalSoftwareWindow>, w: u32, h: u32) {
    let mut buf = vec![slint::Rgb8Pixel::default(); (w * h) as usize];
    for _ in 0..3 {
        CLOCK_MS.fetch_add(50, Ordering::Relaxed);
        slint::platform::update_timers_and_animations();
        win.window().request_redraw();
        win.draw_if_needed(|r| {
            r.render(&mut buf, w as usize);
        });
    }
}

fn selftest(app: &AppWindow, win: &Rc<MinimalSoftwareWindow>, eq: &Rc<RefCell<Eq>>) {
    use slint::LogicalPosition;
    use slint::platform::{Key, PointerEventButton};
    let (w, h) = (1180u32, 760u32);
    fill_base(app, eq);
    let m = app.global::<Model>();
    m.set_theme_mode(1);
    m.set_page(Page::Equalizer);
    win.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
    win.set_size(slint::PhysicalSize::new(w, h));
    settle(win, w, h);

    let log = Rc::new(RefCell::new(Log::default()));
    macro_rules! rec {
        ($on:ident, $name:expr, |$($a:ident),*| $fmt:expr) => {{
            let l = log.clone();
            m.$on(move |$($a),*| l.borrow_mut().events.push(format!("{}:{}", $name, $fmt)));
        }};
    }
    rec!(on_band_drag, "drag", |i, x, y| format!("{i},{x:.3},{y:.3}"));
    rec!(on_band_drag_end, "drag-end", |i| format!("{i}"));
    rec!(on_band_add, "add", |x, y| format!("{x:.3},{y:.3}"));
    rec!(on_band_remove, "remove", |i| format!("{i}"));
    rec!(on_band_width, "width", |i, s| format!("{i},{s:.2}"));
    rec!(on_band_set_on, "set-on", |i, v| format!("{i},{v}"));
    rec!(on_band_nudge, "nudge", |i, f, s| format!("{i},{f},{s}"));
    rec!(on_band_set_freq, "set-freq", |i, t| format!("{i},{t}"));
    rec!(on_undo, "undo", || "");
    rec!(on_redo, "redo", || "");
    rec!(on_set_comparing, "comparing", |v| format!("{v}"));
    rec!(on_slider_changed, "slider", |i, v| format!("{i},{v:.1}"));
    rec!(on_slider_released, "slider-end", |i| format!("{i}"));
    rec!(on_set_enabled, "enabled", |v| format!("{v}"));
    rec!(on_preset_load, "preset-load", |i| format!("{i}"));

    let (ox, oy) = (280.0f32, 144.0f32);
    let (pw, ph) = (eq.borrow().plot.0 as f32, eq.borrow().plot.1 as f32);
    println!("plot size reported by graph-resized: {pw} x {ph}");
    let band = |i: usize| {
        let s = eq.borrow().specs[i];
        (ox + x_pos(s.freq) as f32 * pw, oy + y_pos(if has_gain(s.kind) { s.gain } else { 0.0 }) as f32 * ph)
    };
    let pos = |x: f32, y: f32| LogicalPosition::new(x, y);
    let mv = |x: f32, y: f32| {
        win.dispatch_event(WindowEvent::PointerMoved { position: pos(x, y) });
        settle(win, w, h);
    };
    let down = |x: f32, y: f32| {
        win.dispatch_event(WindowEvent::PointerMoved { position: pos(x, y) });
        win.dispatch_event(WindowEvent::PointerPressed {
            position: pos(x, y),
            button: PointerEventButton::Left,
        });
    };
    let up = |x: f32, y: f32| {
        win.dispatch_event(WindowEvent::PointerReleased {
            position: pos(x, y),
            button: PointerEventButton::Left,
        });
        settle(win, w, h);
    };
    let key = |k: slint::SharedString| {
        win.dispatch_event(WindowEvent::KeyPressed { text: k.clone() });
        win.dispatch_event(WindowEvent::KeyReleased { text: k });
        settle(win, w, h);
    };
    let mut results: Vec<(String, bool, String)> = Vec::new();
    let mut check = |name: &str, ok: bool, detail: String| {
        println!("{} {name}: {detail}", if ok { "PASS" } else { "FAIL" });
        results.push((name.to_string(), ok, detail));
    };
    let take = |log: &Rc<RefCell<Log>>| std::mem::take(&mut log.borrow_mut().events);

    // 1. drag band 1 (index 1) by +60 px right and -40 px up
    let (bx, by) = band(1);
    mv(bx, by);
    down(bx, by);
    mv(bx + 30.0, by - 20.0);
    mv(bx + 60.0, by - 40.0);
    up(bx + 60.0, by - 40.0);
    let ev = take(&log);
    let drags: Vec<&String> = ev.iter().filter(|e| e.starts_with("drag:")).collect();
    let last = drags.last().map(|s| s.to_string()).unwrap_or_default();
    let want_x = (bx + 60.0 - ox) / pw;
    let want_y = (by - 40.0 - oy) / ph;
    let ok = last.starts_with("drag:1,")
        && {
            let parts: Vec<f32> = last[7..].split(',').filter_map(|p| p.parse().ok()).collect();
            parts.len() == 2 && (parts[0] - want_x).abs() < 0.01 && (parts[1] - want_y).abs() < 0.01
        }
        && ev.iter().any(|e| e == "drag-end:1");
    check("drag a handle", ok, format!("last={last} want=({want_x:.3},{want_y:.3}) events={}", ev.len()));
    check("press selects band", m.get_selected_band() == 1, format!("selected={}", m.get_selected_band()));

    // 2. double click empty space adds a band
    let (ex, ey) = (ox + 0.3 * pw, oy + 0.85 * ph);
    mv(ex, ey);
    down(ex, ey);
    up(ex, ey);
    down(ex, ey);
    up(ex, ey);
    let ev = take(&log);
    check(
        "double click empty adds band",
        ev.iter().any(|e| e.starts_with("add:0.300,0.8")),
        format!("{ev:?}"),
    );

    // 3. double click a handle toggles it
    let (hx, hy) = band(3);
    mv(hx, hy);
    down(hx, hy);
    up(hx, hy);
    down(hx, hy);
    up(hx, hy);
    let ev = take(&log);
    check("double click handle toggles", ev.iter().any(|e| e == "set-on:3,false"), format!("{ev:?}"));

    // 4. wheel over a handle changes width, wheel over the graph uses the selected band
    win.dispatch_event(WindowEvent::PointerScrolled { position: pos(hx, hy), delta_x: 0.0, delta_y: 60.0 });
    settle(win, w, h);
    let ev = take(&log);
    check("wheel over handle", ev.iter().any(|e| e.starts_with("width:3,")), format!("{ev:?}"));
    win.dispatch_event(WindowEvent::PointerScrolled {
        position: pos(ox + 0.5 * pw, oy + 0.8 * ph),
        delta_x: 0.0,
        delta_y: -60.0,
    });
    settle(win, w, h);
    let ev = take(&log);
    check("wheel over graph with selection", ev.iter().any(|e| e.starts_with("width:3,")), format!("{ev:?}"));

    // 5. keys with the graph focused
    key(Key::RightArrow.into());
    key(Key::UpArrow.into());
    key(Key::Delete.into());
    let ev = take(&log);
    check(
        "arrows nudge, delete removes",
        ev.iter().any(|e| e == "nudge:3,0,1")
            && ev.iter().any(|e| e == "nudge:3,1,1")
            && ev.iter().any(|e| e == "remove:3"),
        format!("{ev:?}"),
    );

    // 6. undo and redo shortcuts
    win.dispatch_event(WindowEvent::KeyPressed { text: Key::Control.into() });
    key("z".into());
    win.dispatch_event(WindowEvent::KeyPressed { text: Key::Shift.into() });
    key("Z".into());
    win.dispatch_event(WindowEvent::KeyReleased { text: Key::Shift.into() });
    win.dispatch_event(WindowEvent::KeyReleased { text: Key::Control.into() });
    settle(win, w, h);
    let ev = take(&log);
    check(
        "ctrl+z undo, ctrl+shift+z redo",
        ev.iter().any(|e| e == "undo:") && ev.iter().any(|e| e == "redo:"),
        format!("{ev:?}"),
    );

    // 7. compare: press and hold
    let (cx, cy) = (1025.0, 28.0);
    mv(cx, cy);
    down(cx, cy);
    settle(win, w, h);
    let pressed_ev = take(&log);
    CLOCK_MS.fetch_add(800, Ordering::Relaxed);
    settle(win, w, h);
    up(cx, cy);
    let ev = take(&log);
    check(
        "compare hold",
        pressed_ev.iter().any(|e| e == "comparing:true") && ev.iter().any(|e| e == "comparing:false"),
        format!("down={pressed_ev:?} up={ev:?}"),
    );

    // 8. segmented control flips to sliders
    let (sx, sy) = (395.0, 97.0);
    mv(sx, sy);
    down(sx, sy);
    up(sx, sy);
    check("Sliders segment", m.get_slider_view(), format!("slider_view={}", m.get_slider_view()));

    // 9. drag a vertical slider (column 3 of 10)
    let track_x = 580.0f32;
    let track_top = 152.0f32;
    let track_h = 340.0f32;
    let (tx, ty) = (track_x, track_top + track_h * 0.5);
    mv(tx, ty);
    down(tx, ty);
    mv(tx, ty - track_h * 0.25);
    up(tx, ty - track_h * 0.25);
    let ev = take(&log);
    let sl: Vec<&String> = ev.iter().filter(|e| e.starts_with("slider:")).collect();
    check(
        "drag a slider",
        sl.last().map(|s| s.starts_with("slider:3,")).unwrap_or(false)
            && ev.iter().any(|e| e == "slider-end:3"),
        format!("{:?}", sl.last()),
    );
    // double click resets
    down(tx, ty);
    up(tx, ty);
    down(tx, ty);
    up(tx, ty);
    let ev = take(&log);
    check("double click slider resets", ev.iter().any(|e| e == "slider:3,0.0"), format!("{ev:?}"));

    // 10. typed frequency commit
    m.set_slider_view(false);
    m.set_selected_band(2);
    settle(win, w, h);
    let (fx, fy) = (715.0, 713.0);
    mv(fx, fy);
    down(fx, fy);
    up(fx, fy);
    settle(win, w, h);
    for c in ['1', '.', '5', 'k'] {
        key(c.to_string().into());
    }
    key(Key::Return.into());
    let ev = take(&log);
    check(
        "typed frequency commits on Enter",
        ev.iter().any(|e| e.starts_with("set-freq:2,") && e.contains("1.5k")),
        format!("{ev:?}"),
    );

    // 11. preset row click and nav
    m.set_page(Page::Presets);
    settle(win, w, h);
    mv(500.0, 285.0);
    down(500.0, 285.0);
    up(500.0, 285.0);
    let ev = take(&log);
    check("click preset loads it", ev.iter().any(|e| e == "preset-load:1"), format!("{ev:?}"));
    let (nx, ny) = (100.0, 241.0);
    down(nx, ny - 120.0);
    up(nx, ny - 120.0);
    check(
        "nav item switches page",
        m.get_page() == Page::Headphones || m.get_page() == Page::Equalizer,
        format!("page={:?}", m.get_page() as i32),
    );

    let failed = results.iter().filter(|r| !r.1).count();
    println!("{} checks, {} failed", results.len(), failed);
    if failed > 0 {
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

fn write_png(path: &std::path::Path, w: u32, h: u32, buf: &[slint::Rgb8Pixel]) {
    let f = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(f, w, h);
    e.set_color(png::ColorType::Rgb);
    let bytes: Vec<u8> = buf.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    e.write_header().unwrap().write_image_data(&bytes).unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut outdir = std::path::PathBuf::from("shots");
    let (mut f_scene, mut f_theme, mut f_size, mut f_scale) = (None, None, None, None);
    let mut selftest_only = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--scene" => f_scene = it.next().cloned(),
            "--theme" => f_theme = it.next().cloned(),
            "--size" => f_size = it.next().cloned(),
            "--scale" => f_scale = it.next().and_then(|s| s.parse::<f32>().ok()),
            "--selftest" => selftest_only = true,
            other => outdir = std::path::PathBuf::from(other),
        }
    }
    std::fs::create_dir_all(&outdir).unwrap();

    let win = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(win.clone()))).unwrap();
    let app = AppWindow::new().unwrap();
    app.show().unwrap();

    let eq = Rc::new(RefCell::new(Eq {
        specs: vec![],
        selected: -1,
        plot: (900.0, 330.0),
        split_ears: true,
        correction: true,
    }));
    {
        let weak = app.as_weak();
        let eq2 = eq.clone();
        app.global::<Model>().on_graph_resized(move |w, h| {
            if w < 1.0 || h < 1.0 {
                return;
            }
            let mut e = eq2.borrow_mut();
            if (e.plot.0 - w as f64).abs() < 0.5 && (e.plot.1 - h as f64).abs() < 0.5 {
                return;
            }
            e.plot = (w as f64, h as f64);
            if let Some(app) = weak.upgrade() {
                push_eq(&app, &e);
            }
        });
    }

    if selftest_only {
        selftest(&app, &win, &eq);
        return;
    }

    let sizes: Vec<(u32, u32)> = match f_size {
        Some(s) => {
            let (w, h) = s.split_once('x').expect("--size WxH");
            vec![(w.parse().unwrap(), h.parse().unwrap())]
        }
        None => vec![(1180, 760), (900, 600)],
    };
    let themes: Vec<(&str, i32)> = match f_theme.as_deref() {
        Some("dark") => vec![("dark", 1)],
        Some("light") => vec![("light", 2)],
        _ => vec![("dark", 1), ("light", 2)],
    };

    let mut count = 0;
    for scene in scenes() {
        if let Some(f) = &f_scene {
            if !scene.name.contains(f.as_str()) {
                continue;
            }
        }
        for &(tname, tmode) in &themes {
            for &(lw, lh) in &sizes {
                let mut scales = vec![1.0f32];
                if scene.main && lw == 1180 {
                    scales.push(2.0);
                }
                if let Some(s) = f_scale {
                    scales = vec![s];
                }
                for scale in scales {
                    // fresh data every time so scenes never leak into each other
                    fill_base(&app, &eq);
                    let m = app.global::<Model>();
                    m.set_theme_mode(tmode);
                    m.set_page(scene.page);
                    (scene.setup)(&app, &eq);
                    win.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: scale });
                    let (pw, ph) = ((lw as f32 * scale) as u32, (lh as f32 * scale) as u32);
                    win.set_size(slint::PhysicalSize::new(pw, ph));
                    let mut buf = vec![slint::Rgb8Pixel::default(); (pw * ph) as usize];
                    if scene.tabs > 0 || scene.hover.is_some() || scene.click.is_some() {
                        settle(&win, pw, ph);
                        for _ in 0..scene.tabs {
                            win.dispatch_event(WindowEvent::KeyPressed {
                                text: slint::platform::Key::Tab.into(),
                            });
                            win.dispatch_event(WindowEvent::KeyReleased {
                                text: slint::platform::Key::Tab.into(),
                            });
                            settle(&win, pw, ph);
                        }
                        if let Some((cx, cy)) = scene.click {
                            let p = slint::LogicalPosition::new(cx, cy);
                            win.dispatch_event(WindowEvent::PointerMoved { position: p });
                            win.dispatch_event(WindowEvent::PointerPressed {
                                position: p,
                                button: slint::platform::PointerEventButton::Left,
                            });
                            win.dispatch_event(WindowEvent::PointerReleased {
                                position: p,
                                button: slint::platform::PointerEventButton::Left,
                            });
                            settle(&win, pw, ph);
                        }
                        if let Some((hx, hy)) = scene.hover {
                            win.dispatch_event(WindowEvent::PointerMoved {
                                position: slint::LogicalPosition::new(hx, hy),
                            });
                        }
                    }
                    for _ in 0..3 {
                        CLOCK_MS.fetch_add(1000, Ordering::Relaxed);
                        slint::platform::update_timers_and_animations();
                        win.draw_if_needed(|r| {
                            r.render(&mut buf, pw as usize);
                        });
                    }
                    // always draw one last frame, even if nothing is marked dirty
                    win.window().request_redraw();
                    win.draw_if_needed(|r| {
                        r.render(&mut buf, pw as usize);
                    });
                    let suffix = if scale > 1.0 { format!("@{}x", scale as u32) } else { String::new() };
                    let name = format!("{}-{}-{}x{}{}.png", scene.name, tname, lw, lh, suffix);
                    write_png(&outdir.join(name), pw, ph, &buf);
                    count += 1;
                }
            }
        }
    }
    println!("wrote {count} images to {}", outdir.display());
}
