//! Records the README pictures from the real app, with no window, no sound
//! device and a clock that only moves between frames.
//! Usage: cargo run --release --example demo -- <output folder>

use eq_backend::{Action, Backend, Check, Device, Level, State, Status};
use eq_store::Store;
use eqaura::worker::{Event, Immediate};
use eqaura::{AppWindow, Model, Page, app};
use slint::ComponentHandle;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

thread_local! {
    static NOW: Cell<Duration> = const { Cell::new(Duration::from_secs(1)) };
}

struct Headless(Rc<MinimalSoftwareWindow>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
    fn duration_since_start(&self) -> Duration {
        NOW.with(Cell::get)
    }
}

/// Looks like a healthy computer in the pictures.
struct Studio;

impl Backend for Studio {
    fn devices(&mut self) -> Vec<Device> {
        ["Studio headphones", "Built-in speakers", "Living room TV"]
            .iter()
            .enumerate()
            .map(|(i, n)| Device { id: format!("d{i}"), name: (*n).into(), is_default: i == 0 })
            .collect()
    }
    fn apply(&mut self, _: &State) -> Result<(), String> {
        Ok(())
    }
    fn status(&mut self) -> Status {
        Status::new(Level::Good, "Active")
    }
    fn checks(&mut self) -> Vec<Check> {
        vec![
            Check::new(
                Level::Good,
                "The sound engine is active",
                "It keeps working when this window is closed.",
            ),
            Check::new(Level::Good, "Studio headphones", "The equalizer works on this device."),
            Check::new(
                Level::Warning,
                "Living room TV",
                "A driver update removed the equalizer from this device.",
            )
            .with("Repair", Action::OpenDeviceSelector),
        ]
    }
    fn act(&mut self, _: Action) -> Result<String, String> {
        Ok("Repaired".into())
    }
    fn supports_start_at_login(&self) -> bool {
        true
    }
    fn set_start_at_login(&mut self, _: bool) -> Result<(), String> {
        Ok(())
    }
}

struct Recorder {
    window: Rc<MinimalSoftwareWindow>,
    size: (u32, u32),
    scale: f32,
    out: PathBuf,
    frame: usize,
    clip: String,
}

impl Recorder {
    fn step(&self, ms: u64) {
        NOW.with(|n| n.set(n.get() + Duration::from_millis(ms)));
        slint::platform::update_timers_and_animations();
    }

    fn settle(&self) {
        for _ in 0..40 {
            self.step(25);
        }
    }

    fn shot(&self, path: &Path) {
        let (w, h) = ((self.size.0 as f32 * self.scale) as usize, (self.size.1 as f32 * self.scale) as usize);
        let mut buf = vec![slint::Rgb8Pixel::default(); w * h];
        self.window.request_redraw();
        self.window.draw_if_needed(|r| {
            r.render(&mut buf, w);
        });
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut e = png::Encoder::new(std::fs::File::create(path).unwrap(), w as u32, h as u32);
        e.set_color(png::ColorType::Rgb);
        let bytes: Vec<u8> = buf.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        e.write_header().unwrap().write_image_data(&bytes).unwrap();
    }

    fn scale(&mut self, scale: f32) {
        self.scale = scale;
        self.window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: scale });
        self.window.set_size(slint::PhysicalSize::new(
            (self.size.0 as f32 * scale) as u32,
            (self.size.1 as f32 * scale) as u32,
        ));
        self.settle();
    }

    fn clip(&mut self, name: &str) {
        self.clip = name.into();
        self.frame = 0;
        self.settle();
    }

    /// Records `n` frames at 20 per second.
    fn record(&mut self, n: usize) {
        for _ in 0..n {
            self.step(50);
            self.shot(&self.out.join(&self.clip).join(format!("{:04}.png", self.frame)));
            self.frame += 1;
        }
    }
}

fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).expect("output folder"));
    let home = std::env::temp_dir().join(format!("eqaura-demo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    // SAFETY: no other thread is running yet.
    unsafe { std::env::set_var("EQAURA_HOME", &home) };
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    let size = (1180, 760);
    window.set_size(slint::PhysicalSize::new(size.0, size.1));
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    app::start_with(
        &ui,
        Store::at(home.clone()),
        Box::new(Immediate::new(Box::new(Studio), Box::new(app::event))),
    );
    ui.show().unwrap();
    app::event(Event::Devices(Studio.devices()));
    app::event(Event::Status(Studio.status()));
    let m = ui.global::<Model>();
    m.invoke_set_theme_mode(1);
    let mut s = Recorder { window, size, scale: 1.0, out: out.clone(), frame: 0, clip: String::new() };
    s.settle();

    // Clip 1: shaping a curve.
    s.clip("equalizer");
    s.record(12);
    let moves: [(f32, f32, f32); 5] =
        [(0.14, 0.5, 0.20), (0.36, 0.5, 0.64), (0.57, 0.5, 0.42), (0.71, 0.5, 0.74), (0.88, 0.5, 0.27)];
    for (i, (x, from, to)) in moves.into_iter().enumerate() {
        m.invoke_band_add(x, from);
        s.record(2);
        for k in 1..=14 {
            let t = ease(k as f32 / 14.0);
            m.invoke_band_drag(i as i32, x - 0.03 * (1.0 - t), from + (to - from) * t);
            s.record(1);
        }
        m.invoke_band_drag_end(i as i32);
        s.record(3);
    }
    for k in 0..16 {
        m.invoke_band_width(3, if k < 10 { 1.0 } else { -0.5 });
        s.record(1);
    }
    for k in 0..24 {
        let a = k as f32 / 24.0 * std::f32::consts::TAU;
        m.invoke_band_drag(2, 0.57 + 0.10 * a.sin(), 0.42 - 0.22 * (1.0 - a.cos()) / 2.0);
        s.record(1);
    }
    m.invoke_band_drag_end(2);
    s.record(24);

    // Clip 2: plain-English tuning.
    m.invoke_reset();
    m.set_toast("".into());
    m.set_page(Page::Assistant);
    s.clip("assistant");
    s.record(10);
    m.invoke_assistant_send("A bit more bass, and the vocals are harsh".into());
    s.record(34);
    m.invoke_assistant_apply(1);
    s.record(24);
    m.set_page(Page::Equalizer);
    s.record(44);

    // Clip 3: headphone correction.
    m.invoke_reset();
    m.set_toast("".into());
    m.set_page(Page::Headphones);
    s.clip("headphones");
    s.record(10);
    let name = "sennheiser hd 600";
    for k in 1..=name.len() {
        m.set_headphone_query(name[..k].into());
        m.invoke_headphone_search(name[..k].into());
        s.record(2);
    }
    s.record(12);
    m.invoke_headphone_pick(0);
    s.record(26);
    m.set_page(Page::Equalizer);
    s.record(34);

    // Still pictures, twice the size for sharp screens.
    m.invoke_preset_load(4);
    m.invoke_band_add(0.2, 0.36);
    m.invoke_assistant_clear();
    m.invoke_assistant_send("It sounds boomy and a little dull".into());
    m.invoke_health_refresh();
    s.settle();
    m.set_toast("".into());
    s.scale(2.0);
    for (theme, mode) in [("dark", 1), ("light", 2)] {
        m.invoke_set_theme_mode(mode);
        for (name, page) in [
            ("equalizer", Page::Equalizer),
            ("headphones", Page::Headphones),
            ("assistant", Page::Assistant),
            ("presets", Page::Presets),
            ("system", Page::System),
            ("settings", Page::Settings),
        ] {
            m.set_page(page);
            s.settle();
            m.set_toast("".into());
            s.settle();
            s.shot(&out.join("stills").join(format!("{name}-{theme}.png")));
        }
    }
    app::finish();
    let _ = std::fs::remove_dir_all(home);
}
