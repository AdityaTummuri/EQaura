#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eq_store::Store;
use eqaura::{AppWindow, app, worker};
use slint::ComponentHandle;
use std::fs::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = eq_store::default_dir();
    #[cfg(target_os = "macos")]
    if std::env::args().any(|a| a == eq_backend::mac::ENGINE_FLAG) {
        return Ok(eq_backend::mac::run_engine(dir)?);
    }
    if std::env::args().any(|a| a == "--version") {
        println!("EQaura {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    std::fs::create_dir_all(&dir)?;
    // One window at a time: a second start brings the first one to the front.
    let lock = File::create(dir.join("app.lock"))?;
    if matches!(lock.try_lock(), Err(std::fs::TryLockError::WouldBlock)) {
        std::fs::write(dir.join(app::SHOW_FILE), b"")?;
        return Ok(());
    }
    log_panics();
    let _ = slint::set_xdg_app_id("eqaura");
    let window = AppWindow::new()?;
    let demo = std::env::var_os("EQAURA_DEMO").is_some();
    app::start(&window, Store::at(dir.clone()), |events| {
        worker::spawn(move || if demo { Box::new(eq_backend::Demo::new(dir)) } else { eq_backend::system(dir) }, events)
    });
    let result = window.run();
    app::finish();
    Ok(result?)
}

/// Keeps a local record of crashes. Nothing is sent anywhere.
fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        Store::open().log(&format!("crash {info}"));
        default(info);
    }));
}
