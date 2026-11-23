//! Turns curves into drawing data for the graph.

use crate::ui::{Band, GridLine};
use eq_core::curve::{Grid, MAX_HZ, MIN_HZ, log_grid};
use eq_core::filter::Filter;
use eq_core::{profile::Profile, text, warning};
use std::fmt::Write;

const STEPS: [f64; 9] = [6.0, 12.0, 18.0, 24.0, 36.0, 48.0, 72.0, 120.0, 240.0];

pub fn x_of(freq: f64) -> f64 {
    (freq / MIN_HZ).ln() / (MAX_HZ / MIN_HZ).ln()
}

pub fn freq_of(x: f64) -> f64 {
    MIN_HZ * (MAX_HZ / MIN_HZ).powf(x)
}

pub fn y_of(db: f64, range: f64) -> f64 {
    0.5 - db / (2.0 * range)
}

pub fn db_of(y: f64, range: f64) -> f64 {
    (0.5 - y) * 2.0 * range
}

/// Three significant digits: 1234 becomes 1230, 56.78 becomes 56.8.
pub fn round_freq(f: f64) -> f64 {
    if f <= 0.0 || !f.is_finite() {
        return f;
    }
    let scale = 10f64.powf(2.0 - f.log10().floor());
    (f * scale).round() / scale
}

/// The smallest range that shows `db` with a little headroom.
pub fn range_for(db: f64) -> f64 {
    let need = db.abs() * 1.04;
    STEPS.into_iter().skip(1).find(|s| *s >= need).unwrap_or(STEPS[STEPS.len() - 1])
}

pub fn next_range(range: f64) -> f64 {
    STEPS.into_iter().find(|s| *s > range).unwrap_or(range)
}

pub struct Plot {
    pub w: f64,
    pub h: f64,
    pub grid: Grid,
}

impl Plot {
    pub fn new(w: f64, h: f64, rate: f64) -> Self {
        // Two pixels apart is smooth on sharp screens and still cheap.
        let n = ((w / 2.0) as usize).clamp(64, 900);
        Self { w, h, grid: Grid::new(log_grid(MIN_HZ, MAX_HZ, n), rate) }
    }

    /// Values in dB as path data, kept near the plot so that extreme values
    /// stay cheap to draw.
    pub fn path(&self, values: &[f64], range: f64) -> String {
        let n = values.len().max(2) - 1;
        let mut s = String::with_capacity(values.len() * 14);
        for (i, v) in values.iter().enumerate() {
            let x = self.w * i as f64 / n as f64;
            let y = (y_of(*v, range) * self.h).clamp(-self.h * 0.5, self.h * 1.5);
            let _ = write!(s, "{} {x:.1} {y:.1} ", if i == 0 { 'M' } else { 'L' });
        }
        s
    }

    /// The same path closed along the 0 dB line.
    pub fn fill(&self, path: &str, range: f64) -> String {
        let zero = y_of(0.0, range) * self.h;
        format!("{path}L {:.1} {zero:.1} L 0 {zero:.1} Z", self.w)
    }
}

pub fn grid_x() -> Vec<GridLine> {
    let mut out = Vec::new();
    for decade in [10.0, 100.0, 1000.0, 10_000.0] {
        for k in 1..10 {
            let f = decade * f64::from(k);
            if !(MIN_HZ..=MAX_HZ).contains(&f) {
                continue;
            }
            let strong = matches!(k, 1 | 2 | 5);
            let label = match (strong, f >= 1000.0) {
                (false, _) => String::new(),
                (true, true) => format!("{}k", f / 1000.0),
                (true, false) => format!("{f}"),
            };
            out.push(GridLine {
                pos: x_of(f) as f32,
                label: label.into(),
                strong: f == 100.0 || f == 1000.0 || f == 10_000.0,
            });
        }
    }
    out
}

pub fn grid_y(range: f64) -> Vec<GridLine> {
    let step = range / 4.0;
    (-3..=3)
        .map(|k| {
            let db = step * f64::from(k);
            GridLine { pos: y_of(db, range) as f32, label: text::signed(db).into(), strong: k == 0 }
        })
        .collect()
}

pub fn bands(p: &Profile, warnings: &[warning::Warning], range: f64) -> Vec<Band> {
    p.filters
        .iter()
        .enumerate()
        .map(|(i, f)| Band {
            on: f.on,
            kind: kind_index(f),
            channel: eq_core::Channel::ALL.iter().position(|c| *c == f.channel).unwrap_or(0) as i32,
            freq: f.freq_hz as f32,
            gain: f.gain_db as f32,
            q: f.q as f32,
            has_gain: f.kind.has_gain(),
            x: x_of(f.freq_hz).clamp(0.0, 1.0) as f32,
            y: y_of(if f.kind.has_gain() { f.gain_db } else { 0.0 }, range).clamp(0.0, 1.0) as f32,
            warn: warnings.iter().any(|w| w.band == Some(i) && w.severity > warning::Severity::Info),
        })
        .collect()
}

fn kind_index(f: &Filter) -> i32 {
    eq_core::Kind::ALL.iter().position(|k| *k == f.kind).unwrap_or(0) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_round_trip() {
        for f in [20.0, 1000.0, 20_000.0] {
            assert!((freq_of(x_of(f)) / f - 1.0).abs() < 1e-9);
        }
        assert_eq!(db_of(y_of(-7.5, 12.0), 12.0), -7.5);
        assert_eq!(round_freq(1234.0), 1230.0);
        assert_eq!(round_freq(56.78), 56.8);
        assert_eq!(range_for(3.0), 12.0);
        assert_eq!(range_for(12.0), 18.0);
        assert_eq!(range_for(1e9), 240.0);
    }

    #[test]
    fn paths_stay_small_with_extreme_values() {
        let p = Plot::new(800.0, 300.0, 48_000.0);
        let s = p.path(&[f64::MAX, -1e300, 0.0], 12.0);
        assert!(s.len() < 80 && !s.contains("inf") && !s.contains("NaN"));
    }
}
