//! EQaura's platform-independent core. Every number the app shows or sends to
//! an audio engine comes from this crate, so what you see is what you hear.

pub mod autoeq;
pub mod change;
pub mod curve;
pub mod dsp;
pub mod filter;
pub mod hearing;
pub mod history;
pub mod preset;
pub mod profile;
pub mod rules;
pub mod sliders;
pub mod text;
pub mod warning;

pub use change::{Change, apply_changes, describe_change};
pub use filter::{Channel, Coeffs, Filter, Kind};
pub use profile::{Chain, Correction, HearingLayer, Preamp, Profile};
pub use warning::{Severity, Warning};
