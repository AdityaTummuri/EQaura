//! The simple ten-slider view. Sliders are ordinary peaking bands at fixed
//! frequencies, so both views always describe the same sound.

use crate::filter::{Channel, Filter, Kind};

pub const FREQS: [f64; 10] =
    [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
pub const LABELS: [&str; 10] = ["31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];
pub const Q: f64 = 1.41;

fn is_slider(f: &Filter, freq: f64) -> bool {
    f.kind == Kind::Peaking
        && f.channel == Channel::Both
        && (f.freq_hz / freq - 1.0).abs() < 0.005
        && (f.q - Q).abs() < 0.005
}

pub fn position(filters: &[Filter], i: usize) -> f64 {
    filters.iter().find(|f| f.on && is_slider(f, FREQS[i])).map_or(0.0, |f| f.gain_db)
}

/// Sets slider `i`, creating its band when needed.
pub fn set(filters: &mut Vec<Filter>, i: usize, gain_db: f64) {
    if i >= FREQS.len() || !gain_db.is_finite() {
        return;
    }
    if let Some(f) = filters.iter_mut().find(|f| is_slider(f, FREQS[i])) {
        f.gain_db = gain_db;
        f.on = true;
    } else if gain_db != 0.0 {
        filters.push(Filter::peak(FREQS[i], gain_db, Q));
    }
}

/// Drops slider bands that were returned to zero.
pub fn tidy(filters: &mut Vec<Filter>) {
    filters.retain(|f| f.gain_db != 0.0 || !FREQS.iter().any(|x| is_slider(f, *x)));
}
