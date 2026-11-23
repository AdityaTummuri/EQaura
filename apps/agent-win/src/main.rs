#![cfg_attr(windows, windows_subsystem = "windows")]
//! Sits in the Windows tray, notices new output devices and tells the user
//! when the equalizer needs repair. The equalizer itself never needs it.

#[cfg(windows)]
mod tray;

fn main() {
    #[cfg(windows)]
    if let Err(e) = tray::run() {
        eq_store::Store::open().log(&format!("agent: {e}"));
    }
    #[cfg(not(windows))]
    eprintln!("The EQaura tray helper is only needed on Windows.");
}
