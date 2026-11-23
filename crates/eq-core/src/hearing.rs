//! Per-ear hearing test and the gentle compensation made from it.
//! This is listening preference tuning, not a medical test.

use crate::curve;
use crate::filter::Filter;

/// Tested in an order that starts where hearing is easiest.
pub const FREQS: [f64; 8] = [1000.0, 2000.0, 4000.0, 8000.0, 12000.0, 500.0, 250.0, 125.0];
pub const START_DB: f64 = -60.0;
const FLOOR_DB: f64 = -100.0;
const CEIL_DB: f64 = -10.0;
const MAX_TRIALS: u32 = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ear {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tone {
    pub ear: Ear,
    pub freq: f64,
    /// Level relative to full scale.
    pub db: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Threshold {
    pub freq: f64,
    /// Quietest level heard; `None` when even the loudest tone was missed.
    pub left: Option<f64>,
    pub right: Option<f64>,
}

/// Down 10 dB when heard, up 5 dB when missed. A level heard twice on the way
/// up is the threshold.
#[derive(Clone, Debug)]
struct Staircase {
    db: f64,
    ascending: bool,
    hits: Vec<(i32, u32)>,
    trials: u32,
    lowest_heard: Option<f64>,
}

impl Staircase {
    fn new() -> Self {
        Self { db: START_DB, ascending: false, hits: Vec::new(), trials: 0, lowest_heard: None }
    }

    /// Returns `Some(threshold)` when finished.
    fn answer(&mut self, heard: bool) -> Option<Option<f64>> {
        self.trials += 1;
        if heard {
            self.lowest_heard = Some(self.lowest_heard.map_or(self.db, |l| l.min(self.db)));
            if self.ascending {
                let key = self.db.round() as i32;
                let count = match self.hits.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, c)) => {
                        *c += 1;
                        *c
                    }
                    None => {
                        self.hits.push((key, 1));
                        1
                    }
                };
                if count >= 2 {
                    return Some(Some(self.db));
                }
            }
            if self.db <= FLOOR_DB {
                return Some(Some(FLOOR_DB));
            }
            self.db = (self.db - 10.0).max(FLOOR_DB);
            self.ascending = false;
        } else {
            if self.db >= CEIL_DB {
                return Some(self.lowest_heard);
            }
            self.db = (self.db + 5.0).min(CEIL_DB);
            self.ascending = true;
        }
        (self.trials >= MAX_TRIALS).then_some(self.lowest_heard)
    }
}

#[derive(Clone, Debug)]
pub struct Test {
    index: usize,
    ear: Ear,
    stairs: Staircase,
    results: Vec<Threshold>,
    done: bool,
}

impl Default for Test {
    fn default() -> Self {
        Self::new()
    }
}

impl Test {
    pub fn new() -> Self {
        Self {
            index: 0,
            ear: Ear::Left,
            stairs: Staircase::new(),
            results: FREQS.iter().map(|f| Threshold { freq: *f, left: None, right: None }).collect(),
            done: false,
        }
    }

    pub fn tone(&self) -> Option<Tone> {
        (!self.done).then(|| Tone { ear: self.ear, freq: FREQS[self.index], db: self.stairs.db })
    }

    pub fn answer(&mut self, heard: bool) {
        if self.done {
            return;
        }
        let Some(threshold) = self.stairs.answer(heard) else { return };
        let r = &mut self.results[self.index];
        match self.ear {
            Ear::Left => r.left = threshold,
            Ear::Right => r.right = threshold,
        }
        self.stairs = Staircase::new();
        self.index += 1;
        if self.index == FREQS.len() {
            self.index = 0;
            match self.ear {
                Ear::Left => self.ear = Ear::Right,
                Ear::Right => self.done = true,
            }
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn progress(&self) -> f64 {
        if self.done {
            return 1.0;
        }
        let ear = if self.ear == Ear::Right { FREQS.len() } else { 0 };
        (ear + self.index) as f64 / (2 * FREQS.len()) as f64
    }

    /// Thresholds sorted by frequency.
    pub fn thresholds(&self) -> Vec<Threshold> {
        let mut r = self.results.clone();
        r.sort_by(|a, b| a.freq.total_cmp(&b.freq));
        r
    }
}

/// Normal hearing threshold relative to 1 kHz (ISO 226 shape), in dB.
fn normal_offset(freq: f64) -> f64 {
    const T: [(f64, f64); 8] = [
        (125.0, 19.7),
        (250.0, 9.0),
        (500.0, 2.0),
        (1000.0, 0.0),
        (2000.0, -3.7),
        (4000.0, -7.8),
        (8000.0, 10.2),
        (12000.0, 9.9),
    ];
    T.iter().find(|(f, _)| (*f - freq).abs() < 1.0).map_or(0.0, |(_, v)| *v)
}

/// Desired boost per ear at each frequency: half of the measured loss.
pub fn compensation(thresholds: &[Threshold]) -> Vec<(f64, f64, f64)> {
    // The better ear at the best mid frequency is the reference, because the
    // absolute playback level is unknown.
    let reference = thresholds
        .iter()
        .filter(|t| (500.0..=4000.0).contains(&t.freq))
        .flat_map(|t| [t.left, t.right].into_iter().flatten().map(|v| v - normal_offset(t.freq)))
        .fold(f64::INFINITY, f64::min);
    let gain = |t: &Threshold, v: Option<f64>| -> f64 {
        match v {
            Some(v) if reference.is_finite() => {
                let loss = v - normal_offset(t.freq) - reference;
                // Differences below 5 dB are within the test's own error.
                if loss < 5.0 { 0.0 } else { loss * 0.5 }
            }
            // A tone that was never heard cannot be fixed with an equalizer.
            _ => 0.0,
        }
    };
    thresholds.iter().map(|t| (t.freq, gain(t, t.left), gain(t, t.right))).collect()
}

/// Peaking filters whose combined curve passes through the target points.
pub fn fit(points: &[(f64, f64)], rate: f64) -> Vec<Filter> {
    const Q: f64 = 1.2;
    let mut filters: Vec<Filter> = points.iter().map(|(f, g)| Filter::peak(*f, *g, Q)).collect();
    if points.iter().all(|(_, g)| *g == 0.0) {
        return Vec::new();
    }
    for _ in 0..60 {
        let errors: Vec<f64> =
            points.iter().map(|(f, g)| g - curve::response_at(&filters, *f, rate)).collect();
        for (f, e) in filters.iter_mut().zip(&errors) {
            f.gain_db += e * 0.6;
        }
    }
    for f in &mut filters {
        f.gain_db = (f.gain_db * 10.0).round() / 10.0;
    }
    filters.retain(|f| f.gain_db != 0.0);
    filters
}

/// One short, softly faded tone burst as interleaved stereo samples.
pub fn render_tone(tone: Tone, rate: f64) -> Vec<f32> {
    let pulses = 3;
    let on = (0.35 * rate) as usize;
    let off = (0.2 * rate) as usize;
    let fade = 0.04 * rate;
    let amp = 10f64.powf(tone.db / 20.0);
    let mut out = Vec::with_capacity(pulses * (on + off) * 2);
    for _ in 0..pulses {
        for i in 0..on {
            let t = i as f64;
            let edge = (t / fade).min((on as f64 - 1.0 - t) / fade).clamp(0.0, 1.0);
            let env = 0.5 - 0.5 * (std::f64::consts::PI * edge).cos();
            let v = (amp * env * (2.0 * std::f64::consts::PI * tone.freq * t / rate).sin()) as f32;
            match tone.ear {
                Ear::Left => out.extend([v, 0.0]),
                Ear::Right => out.extend([0.0, v]),
            }
        }
        out.resize(out.len() + off * 2, 0.0);
    }
    out
}
