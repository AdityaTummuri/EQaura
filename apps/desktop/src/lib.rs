//! EQaura's desktop app: connects the interface to the core, the audio
//! engines, storage and the assistant.

// Generated code is not ours to lint.
#[allow(clippy::all, clippy::pedantic)]
mod ui {
    slint::include_modules!();
}

mod about;
pub mod app;
mod graph;
mod presets;
mod tone;
pub mod worker;

pub use ui::*;
