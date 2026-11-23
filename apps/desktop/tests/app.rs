//! Drives the whole app without a window, a sound device or the internet.

use eq_backend::{Action, Backend, Check, Demo, Device, Level, State, Status};
use eq_store::Store;
use eqaura::worker::{Event, Immediate};
use eqaura::{AppWindow, Model, Page, Tone, app};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, Model as _};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Headless(Rc<MinimalSoftwareWindow>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

#[derive(Clone, Default)]
struct Spy(Arc<Mutex<State>>);

impl Backend for Spy {
    fn devices(&mut self) -> Vec<Device> {
        Demo::new(Default::default()).devices()
    }
    fn apply(&mut self, state: &State) -> Result<(), String> {
        *self.0.lock().unwrap() = state.clone();
        Ok(())
    }
    fn status(&mut self) -> Status {
        Status::new(Level::Good, "Active")
    }
    fn checks(&mut self) -> Vec<Check> {
        vec![Check::new(Level::Warning, "Test", "Detail").with("Fix", Action::RestartEngine)]
    }
    fn act(&mut self, _: Action) -> Result<String, String> {
        Ok("Fixed".into())
    }
    fn supports_start_at_login(&self) -> bool {
        true
    }
    fn set_start_at_login(&mut self, _: bool) -> Result<(), String> {
        Ok(())
    }
}

fn settle() {
    std::thread::sleep(Duration::from_millis(30));
    slint::platform::update_timers_and_animations();
}

#[test]
fn everything_a_user_can_do() {
    let dir = std::env::temp_dir().join(format!("eqaura-app-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // SAFETY: the only test in this file; no other thread reads the environment.
    unsafe { std::env::set_var("EQAURA_HOME", &dir) };
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    window.set_size(slint::PhysicalSize::new(1180, 760));
    slint::platform::set_platform(Box::new(Headless(window))).unwrap();
    let ui = AppWindow::new().unwrap();
    let spy = Spy::default();
    let heard = spy.0.clone();
    let begin = |spy: Spy| {
        app::start_with(
            &ui,
            Store::at(dir.clone()),
            Box::new(Immediate::new(Box::new(spy.clone()), Box::new(app::event))),
        );
        app::event(Event::Devices(spy.clone().devices()));
        app::event(Event::Status(spy.clone().status()));
        settle();
    };
    begin(spy.clone());
    let m = ui.global::<Model>();
    let sound = || heard.lock().unwrap().devices.get("demo-headphones").cloned().unwrap_or_default();

    // Device and status reach the window.
    assert_eq!(m.get_device_name(), "Headphones");
    assert_eq!(m.get_engine_status(), "Active");
    m.invoke_graph_resized(900.0, 320.0);
    assert!(m.get_curve_total().starts_with("M 0.0 160.0"));

    // Add, drag, type, undo, redo.
    m.invoke_band_add(0.5, 0.25);
    assert_eq!(m.get_bands().row_count(), 1);
    assert_eq!(sound().filters[0].gain_db, 6.0);
    for i in 0..50 {
        m.invoke_band_drag(0, 0.5, 0.25 - i as f32 * 0.004);
    }
    settle();
    m.invoke_band_drag_end(0);
    assert!(sound().filters[0].gain_db > 9.0);
    assert!(m.get_auto_preamp() && m.get_preamp() < -9.0 && !m.get_clipping());
    m.invoke_undo();
    assert_eq!(sound().filters[0].gain_db, 6.0, "one undo step per drag");
    m.invoke_redo();
    assert!(sound().filters[0].gain_db > 9.0);
    m.invoke_band_set_freq(0, "1.5k".into());
    assert_eq!(sound().filters[0].freq_hz, 1500.0);
    m.invoke_band_set_freq(0, "hello".into());
    m.invoke_band_set_freq(0, "-5".into());
    m.invoke_band_set_q(0, "0".into());
    assert_eq!(sound().filters[0].freq_hz, 1500.0);

    // No limits, only warnings.
    m.invoke_band_set_gain(0, "40".into());
    assert_eq!(sound().filters[0].gain_db, 40.0);
    assert!(m.get_warnings().row_count() >= 1);
    assert!(m.get_bands().row_data(0).unwrap().warn);
    m.invoke_set_auto_preamp(false);
    m.invoke_set_preamp("0".into());
    assert!(m.get_clipping());
    m.invoke_set_auto_preamp(true);
    assert!(!m.get_clipping());

    // Bad indexes and odd numbers are ignored.
    for i in [-5, 999, i32::MAX] {
        m.invoke_band_drag(i, 0.5, 0.5);
        m.invoke_band_remove(i);
        m.invoke_band_set_kind(i, 1);
        m.invoke_band_set_kind(0, i);
        m.invoke_band_set_channel(0, i);
        m.invoke_preset_load(i);
        m.invoke_preset_delete(i);
        m.invoke_preset_rename(i);
        m.invoke_headphone_pick(i.saturating_mul(1000));
        m.invoke_assistant_apply(i);
        m.invoke_assistant_dismiss(i);
        m.invoke_health_action(i);
        m.invoke_slider_changed(i, 3.0);
        m.invoke_set_provider(i);
    }
    m.invoke_band_drag(0, f32::NAN, f32::INFINITY);
    m.invoke_band_drag(0, -40.0, 900.0);
    settle();
    m.invoke_band_drag_end(0);
    assert!(sound().filters.iter().all(eq_core::Filter::is_valid));

    // Left and right.
    m.invoke_band_set_channel(0, 2);
    assert!(!m.get_curve_right().is_empty());
    m.invoke_band_set_channel(0, 0);
    assert!(m.get_curve_right().is_empty());

    // Compare keeps the volume and drops the filters.
    let preamp = m.get_preamp();
    m.invoke_set_comparing(true);
    assert!(sound().filters.is_empty() && (sound().preamp.db as f32 - preamp).abs() < 0.02);
    m.invoke_set_comparing(false);
    assert_eq!(sound().filters.len(), 1);

    // Power.
    m.invoke_set_enabled(false);
    assert!(!heard.lock().unwrap().enabled);
    m.invoke_set_enabled(true);
    assert!(heard.lock().unwrap().enabled);

    // Presets.
    let builtins = m.get_presets().row_count() as i32;
    m.invoke_preset_load(1);
    assert!(!m.get_profile_dirty());
    assert_eq!(m.get_profile_name(), "Bass lift");
    m.invoke_slider_changed(0, 2.0);
    settle();
    m.invoke_slider_released(0);
    assert!(m.get_profile_dirty());
    m.invoke_preset_save();
    assert!(m.get_name_dialog());
    m.invoke_name_dialog_accept("  ".into());
    m.invoke_name_dialog_accept("Bass lift".into());
    assert!(m.get_name_dialog(), "built-in names stay taken");
    m.invoke_name_dialog_accept("Mine".into());
    assert!(!m.get_name_dialog() && !m.get_profile_dirty());
    assert_eq!(m.get_presets().row_count() as i32, builtins + 1);
    m.invoke_preset_rename(builtins);
    m.invoke_name_dialog_accept("Yours".into());
    assert_eq!(m.get_profile_name(), "Yours");
    m.invoke_preset_delete(0);
    m.invoke_preset_delete(builtins);
    assert_eq!(m.get_presets().row_count() as i32, builtins);

    // Headphones.
    m.invoke_headphone_search("hd 600".into());
    assert!(m.get_headphone_matches() > 0 && m.get_headphone_total() > 8000);
    m.invoke_headphone_pick(0);
    assert!(m.get_headphone_active().contains("HD 600"));
    assert!(!m.get_curve_correction().is_empty());
    m.invoke_headphone_set_strength(0.5);
    assert_eq!(sound().headphone.unwrap().strength, 0.5);
    m.invoke_headphone_search("no such headphone".into());
    assert_eq!(m.get_headphone_matches(), 0);
    m.invoke_headphone_clear();
    assert!(sound().headphone.is_none());

    // Assistant with the built-in rules.
    m.invoke_reset();
    m.invoke_assistant_send("".into());
    assert_eq!(m.get_chat().row_count(), 0);
    m.invoke_assistant_send("a lot more bass, less harsh".into());
    let row = m.get_chat().row_data(1).unwrap();
    assert_eq!((row.state, row.tone), (1, Tone::Neutral));
    assert!(row.changes.row_count() >= 2);
    assert!(sound().filters.is_empty(), "nothing changes before Apply");
    m.invoke_assistant_apply(1);
    assert!(sound().filters.len() >= 2);
    assert_eq!(m.get_chat().row_data(1).unwrap().state, 2);
    m.invoke_undo();
    assert_eq!(m.get_chat().row_data(1).unwrap().state, 4);
    m.invoke_redo();
    assert_eq!(m.get_chat().row_data(1).unwrap().state, 2);
    m.invoke_assistant_send("what time is it".into());
    assert_eq!(m.get_chat().row_data(3).unwrap().state, 0);
    m.invoke_assistant_clear();
    assert_eq!(m.get_chat().row_count(), 0);

    // System page.
    m.invoke_health_refresh();
    settle();
    assert_eq!(m.get_health().row_data(0).unwrap().action, "Fix");
    assert_eq!(m.get_devices().row_count(), 2);

    // Switching device keeps each device's own sound.
    let bands = sound().filters.len();
    let swap = |default: &str| {
        let mut d = Spy::default().devices();
        d.iter_mut().for_each(|d| d.is_default = d.id == default);
        app::event(Event::Devices(d));
        settle();
    };
    swap("demo-speakers");
    assert_eq!(m.get_bands().row_count(), 0);
    m.invoke_band_add_default();
    swap("demo-headphones");
    assert_eq!(m.get_bands().row_count(), bands);
    app::event(Event::Devices(Vec::new()));
    settle();
    assert_eq!(m.get_device_name(), "No output device");
    m.invoke_band_add_default();
    swap("demo-headphones");
    assert_eq!(m.get_bands().row_count(), bands);

    // Settings.
    m.invoke_set_theme_mode(1);
    m.invoke_set_provider(1);
    assert!(m.get_provider_needs_key() && !m.get_provider_has_key());
    m.invoke_set_provider_key("sk-secret".into());
    assert!(m.get_provider_has_key());
    m.invoke_set_provider_model("some-model".into());
    assert!(m.get_assistant_sends().contains("Output device"));
    m.invoke_clear_provider_key();
    assert!(!m.get_provider_has_key());
    m.invoke_set_provider(0);
    m.set_page(Page::About);

    // Everything is still there after a restart, and no file holds the key.
    app::finish();
    for f in std::fs::read_dir(&dir).unwrap().flatten().filter(|f| f.path().is_file()) {
        assert!(!std::fs::read_to_string(f.path()).unwrap_or_default().contains("sk-secret"));
    }
    begin(spy);
    assert_eq!(m.get_bands().row_count(), bands);
    assert_eq!(m.get_theme_mode(), 1);
    app::finish();
    let _ = std::fs::remove_dir_all(&dir);
}
