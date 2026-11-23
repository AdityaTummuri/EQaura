//! Filter definitions and RBJ Audio EQ Cookbook coefficients.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    Notch,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Peaking,
        Kind::LowShelf,
        Kind::HighShelf,
        Kind::LowPass,
        Kind::HighPass,
        Kind::Notch,
    ];

    pub fn has_gain(self) -> bool {
        matches!(self, Kind::Peaking | Kind::LowShelf | Kind::HighShelf)
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Peaking => "Peak",
            Kind::LowShelf => "Low shelf",
            Kind::HighShelf => "High shelf",
            Kind::LowPass => "Low pass",
            Kind::HighPass => "High pass",
            Kind::Notch => "Notch",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    #[default]
    Both,
    Left,
    Right,
}

impl Channel {
    pub const ALL: [Channel; 3] = [Channel::Both, Channel::Left, Channel::Right];
}

fn yes() -> bool {
    true
}

fn default_q() -> f64 {
    0.71
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    #[serde(rename = "type")]
    pub kind: Kind,
    pub freq_hz: f64,
    #[serde(default)]
    pub gain_db: f64,
    #[serde(default = "default_q")]
    pub q: f64,
    #[serde(default = "yes")]
    pub on: bool,
    #[serde(default)]
    pub channel: Channel,
}

impl Filter {
    pub fn new(kind: Kind, freq_hz: f64, gain_db: f64, q: f64) -> Self {
        Self { kind, freq_hz, gain_db, q, on: true, channel: Channel::Both }
    }

    pub fn peak(freq_hz: f64, gain_db: f64, q: f64) -> Self {
        Self::new(Kind::Peaking, freq_hz, gain_db, q)
    }

    /// A filter whose numbers can be turned into a working filter. This is a
    /// mathematical requirement, not a taste limit.
    pub fn is_valid(&self) -> bool {
        self.freq_hz.is_finite()
            && self.freq_hz > 0.0
            && self.q.is_finite()
            && self.q > 0.0
            && self.gain_db.is_finite()
    }

    /// True when the filter changes the sound at all.
    pub fn is_active(&self) -> bool {
        self.on && self.is_valid() && (!self.kind.has_gain() || self.gain_db != 0.0)
    }

    pub fn scaled(mut self, strength: f64) -> Self {
        self.gain_db *= strength;
        self
    }

    /// The frequency to design at, or `None` when the filter's whole effect
    /// lies above what this sample rate can carry.
    pub fn usable_freq(&self, rate: f64) -> Option<f64> {
        let top = rate / 2.0 * 0.999;
        if self.freq_hz < top {
            return Some(self.freq_hz);
        }
        matches!(self.kind, Kind::LowShelf | Kind::HighPass).then_some(top)
    }

    /// Coefficients at `rate` Hz. Anything that cannot work at this rate
    /// becomes a pass-through rather than an unstable filter.
    pub fn coeffs(&self, rate: f64) -> Coeffs {
        if !self.is_active() || !rate.is_finite() || rate <= 0.0 {
            return Coeffs::IDENTITY;
        }
        let Some(freq) = self.usable_freq(rate) else { return Coeffs::IDENTITY };
        let w0 = 2.0 * PI * freq / rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * self.q);
        let a = 10f64.powf(self.gain_db / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match self.kind {
            Kind::Peaking => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            Kind::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + s),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - s),
                    (a + 1.0) + (a - 1.0) * cos + s,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - s,
                )
            }
            Kind::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + s),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - s),
                    (a + 1.0) - (a - 1.0) * cos + s,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - s,
                )
            }
            Kind::LowPass => (
                (1.0 - cos) / 2.0,
                1.0 - cos,
                (1.0 - cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            Kind::HighPass => (
                (1.0 + cos) / 2.0,
                -(1.0 + cos),
                (1.0 + cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            Kind::Notch => (1.0, -2.0 * cos, 1.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        };
        let c = Coeffs { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
        if c.is_stable() { c } else { Coeffs::IDENTITY }
    }
}

/// Normalised biquad coefficients (a0 = 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Coeffs {
    pub const IDENTITY: Coeffs = Coeffs { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    /// Finite, with both poles inside the unit circle.
    pub fn is_stable(&self) -> bool {
        [self.b0, self.b1, self.b2, self.a1, self.a2].iter().all(|v| v.is_finite())
            && self.a2.abs() < 1.0
            && self.a1.abs() < 1.0 + self.a2
    }

    /// Power ratio at `phi` = sin^2(w / 2). This form keeps its precision at
    /// low frequencies, where the usual cosine form cancels.
    pub fn power(&self, phi: f64) -> f64 {
        let part = |c0: f64, c1: f64, c2: f64| {
            let sum = c0 + c1 + c2;
            let v = sum * sum - 4.0 * (c0 * c1 + 4.0 * c0 * c2 + c1 * c2) * phi
                + 16.0 * c0 * c2 * phi * phi;
            v.max(1e-30)
        };
        part(self.b0, self.b1, self.b2) / part(1.0, self.a1, self.a2)
    }

    pub fn gain_db(&self, phi: f64) -> f64 {
        10.0 * self.power(phi).log10()
    }

    pub fn gain_db_at(&self, freq: f64, rate: f64) -> f64 {
        self.gain_db(phi(freq, rate))
    }
}

/// sin^2(w / 2) for a frequency at a sample rate.
pub fn phi(freq: f64, rate: f64) -> f64 {
    (PI * freq / rate).sin().powi(2)
}
