//! Real-time processing, used by the macOS engine and by accuracy tests.

use crate::filter::Coeffs;
use crate::profile::Chain;

#[derive(Clone, Copy, Debug)]
struct Biquad {
    c: Coeffs,
    z1: f64,
    z2: f64,
}

impl Biquad {
    #[inline(always)]
    fn tick(&mut self, x: f64) -> f64 {
        let y = self.c.b0 * x + self.z1;
        self.z1 = self.c.b1 * x - self.c.a1 * y + self.z2;
        self.z2 = self.c.b2 * x - self.c.a2 * y;
        y
    }
}

/// One channel's filters, ready to run. Never allocates while processing.
#[derive(Clone, Debug)]
pub struct Line {
    stages: Vec<Biquad>,
    gain: f64,
}

impl Line {
    pub fn new(filters: &[crate::filter::Filter], preamp_db: f64, rate: f64) -> Self {
        let stages = filters
            .iter()
            .filter(|f| f.is_active())
            .map(|f| f.coeffs(rate))
            .filter(|c| *c != Coeffs::IDENTITY)
            .map(|c| Biquad { c, z1: 0.0, z2: 0.0 })
            .collect();
        Self { stages, gain: 10f64.powf(preamp_db / 20.0) }
    }

    /// Filters `input` into `out` stage by stage (cache and SIMD friendly).
    pub fn process(&mut self, buf: &mut [f64]) {
        for x in buf.iter_mut() {
            *x *= self.gain;
        }
        for s in &mut self.stages {
            let mut st = *s;
            for x in buf.iter_mut() {
                *x = st.tick(*x);
            }
            // Stop denormals and recover from a bad input sample.
            if !(st.z1.is_finite() && st.z2.is_finite()) {
                st.z1 = 0.0;
                st.z2 = 0.0;
            }
            if st.z1.abs() < 1e-30 {
                st.z1 = 0.0;
            }
            if st.z2.abs() < 1e-30 {
                st.z2 = 0.0;
            }
            *s = st;
        }
    }
}

#[derive(Clone, Debug)]
pub struct Stereo {
    pub left: Line,
    pub right: Line,
}

impl Stereo {
    pub fn new(chain: &Chain, rate: f64) -> Self {
        Self {
            left: Line::new(&chain.left, chain.preamp_db, rate),
            right: Line::new(&chain.right, chain.preamp_db, rate),
        }
    }

    /// Continues from the running filters' memory, which keeps changes smooth
    /// while dragging. Only possible when the filter count is the same.
    pub fn inherit(&mut self, old: &Stereo) {
        for (new, old) in [(&mut self.left, &old.left), (&mut self.right, &old.right)] {
            if new.stages.len() == old.stages.len() {
                for (n, o) in new.stages.iter_mut().zip(&old.stages) {
                    n.z1 = o.z1;
                    n.z2 = o.z2;
                }
            }
        }
    }

    pub fn bypass() -> Self {
        Self { left: Line::new(&[], 0.0, 48_000.0), right: Line::new(&[], 0.0, 48_000.0) }
    }
}
