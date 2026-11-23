//! Frequency responses and automatic preamp.

use crate::filter::{Coeffs, Filter, Kind, phi};
use std::f64::consts::PI;

pub const MIN_HZ: f64 = 20.0;
pub const MAX_HZ: f64 = 20_000.0;

/// `n` frequencies spaced evenly on a log scale.
pub fn log_grid(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    let n = n.max(2);
    let (l, h) = (lo.ln(), hi.ln());
    (0..n).map(|i| (l + (h - l) * i as f64 / (n - 1) as f64).exp()).collect()
}

/// Reusable evaluation points: avoids recomputing sines on every redraw.
#[derive(Clone, Debug)]
pub struct Grid {
    pub freqs: Vec<f64>,
    phi: Vec<f64>,
    rate: f64,
}

impl Grid {
    pub fn new(freqs: Vec<f64>, rate: f64) -> Self {
        let phi = freqs.iter().map(|f| phi(*f, rate)).collect();
        Self { freqs, phi, rate }
    }

    pub fn response(&self, filters: &[Filter]) -> Vec<f64> {
        let mut out = vec![0.0; self.freqs.len()];
        for c in active(filters, self.rate) {
            for (o, phi) in out.iter_mut().zip(&self.phi) {
                *o += c.gain_db(*phi);
            }
        }
        out
    }
}

fn active(filters: &[Filter], rate: f64) -> impl Iterator<Item = Coeffs> + '_ {
    filters.iter().filter(|f| f.is_active()).map(move |f| f.coeffs(rate))
}

/// Combined gain of all filters at one frequency.
pub fn response_at(filters: &[Filter], freq: f64, rate: f64) -> f64 {
    active(filters, rate).map(|c| c.gain_db_at(freq, rate)).sum()
}

/// The highest point of the combined curve anywhere up to Nyquist, in dB.
pub fn peak_db(filters: &[Filter], rate: f64) -> f64 {
    let coeffs: Vec<Coeffs> = active(filters, rate).collect();
    if coeffs.is_empty() {
        return 0.0;
    }
    // Compare power ratios; one logarithm at the end is enough.
    let at = |f: f64| -> f64 {
        let x = phi(f, rate);
        coeffs.iter().map(|c| c.power(x)).product()
    };
    let top = rate / 2.0;
    let mut freqs = log_grid(0.5, top * 0.9999, 400);
    freqs.push(top * 0.999_999);
    // Narrow filters can hide between grid points, so sample around every
    // place where one can have a peak: its centre, and for shelves the
    // resonances on either side of it.
    for f in filters.iter().filter(|f| f.is_active()) {
        let Some(design) = f.usable_freq(rate) else { continue };
        let shift = 10f64.powf(f.gain_db.abs() / 80.0);
        let warped = (PI * design / rate).tan();
        let spots = match f.kind {
            Kind::LowShelf | Kind::HighShelf => [1.0, shift, 1.0 / shift],
            _ => [1.0, 1.0, 1.0],
        };
        let n = if spots[1] == 1.0 { 1 } else { 3 };
        for spot in &spots[..n] {
            // Work in the warped domain, where widths stay true up to Nyquist.
            let centre = warped * spot;
            let unwarp = |x: f64| x.atan() * rate / PI;
            freqs.push(unwarp(centre));
            let mut octaves = (1.0 / f.q).min(2.0);
            for _ in 0..8 {
                let k = 2f64.powf(octaves);
                freqs.extend([unwarp(centre * k), unwarp(centre / k)]);
                octaves /= 2.0;
            }
        }
    }
    freqs.retain(|f| *f > 0.0 && *f < top);
    freqs.sort_unstable_by(f64::total_cmp);
    let values: Vec<f64> = freqs.iter().map(|f| at(*f)).collect();
    let mut best = at(1e-3).max(1.0);
    for i in 0..values.len() {
        best = best.max(values[i]);
        let lower = |j: Option<usize>| j.and_then(|j| values.get(j)).is_none_or(|v| *v <= values[i]);
        if !lower(i.checked_sub(1)) || !lower(Some(i + 1)) {
            continue;
        }
        // Golden-section search inside the bracket.
        let mut lo = freqs[i.saturating_sub(1)].ln();
        let mut hi = freqs[(i + 1).min(freqs.len() - 1)].ln();
        const R: f64 = 0.618_033_988_749_895;
        for _ in 0..24 {
            let m1 = hi - R * (hi - lo);
            let m2 = lo + R * (hi - lo);
            if at(m1.exp()) < at(m2.exp()) { lo = m1 } else { hi = m2 }
        }
        best = best.max(at(((lo + hi) / 2.0).exp()));
    }
    10.0 * best.log10()
}

/// Preamp that keeps the loudest point at or below 0 dB, rounded down to 0.1 dB.
pub fn auto_preamp(peak: f64) -> f64 {
    if peak <= 0.0 { 0.0 } else { -((peak * 10.0 - 1e-6).ceil() / 10.0) }
}
