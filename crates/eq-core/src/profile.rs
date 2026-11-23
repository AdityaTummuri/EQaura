//! A complete sound profile: the user's bands plus correction layers.

use crate::curve;
use crate::filter::{Channel, Filter};
use serde::{Deserialize, Serialize};

pub const SCHEMA: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preamp {
    pub auto: bool,
    /// Used when `auto` is off.
    pub db: f64,
}

impl Default for Preamp {
    fn default() -> Self {
        Self { auto: true, db: 0.0 }
    }
}

fn yes() -> bool {
    true
}
fn one() -> f64 {
    1.0
}

/// Headphone correction from the AutoEq database.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Correction {
    pub name: String,
    #[serde(default)]
    pub source: String,
    #[serde(default = "yes")]
    pub on: bool,
    #[serde(default = "one")]
    pub strength: f64,
    pub filters: Vec<Filter>,
}

/// Per-ear compensation from the hearing test.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HearingLayer {
    #[serde(default = "yes")]
    pub on: bool,
    #[serde(default = "one")]
    pub strength: f64,
    pub left: Vec<Filter>,
    pub right: Vec<Filter>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub schema: u32,
    pub name: String,
    #[serde(default)]
    pub preamp: Preamp,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headphone: Option<Correction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hearing: Option<HearingLayer>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            name: "Untitled".into(),
            preamp: Preamp::default(),
            filters: Vec::new(),
            headphone: None,
            hearing: None,
        }
    }
}

/// What actually runs: one filter list per ear, plus the preamp.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chain {
    pub left: Vec<Filter>,
    pub right: Vec<Filter>,
    pub preamp_db: f64,
    /// Highest point of the curve before the preamp.
    pub peak_db: f64,
}

impl Chain {
    pub fn is_symmetric(&self) -> bool {
        self.left == self.right
    }

    /// Highest point after the preamp; above 0 dB can clip.
    pub fn net_peak_db(&self) -> f64 {
        self.peak_db + self.preamp_db
    }
}

impl Profile {
    fn split(&self, with_user: bool) -> (Vec<Filter>, Vec<Filter>) {
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut push = |f: Filter| {
            if !f.is_active() {
                return;
            }
            let plain = Filter { channel: Channel::Both, ..f };
            match f.channel {
                Channel::Both => {
                    left.push(plain);
                    right.push(plain);
                }
                Channel::Left => left.push(plain),
                Channel::Right => right.push(plain),
            }
        };
        if let Some(h) = self.headphone.as_ref().filter(|h| h.on) {
            h.filters.iter().for_each(|f| push(f.scaled(h.strength)));
        }
        if let Some(h) = self.hearing.as_ref().filter(|h| h.on) {
            for f in &h.left {
                push(Filter { channel: Channel::Left, ..f.scaled(h.strength) });
            }
            for f in &h.right {
                push(Filter { channel: Channel::Right, ..f.scaled(h.strength) });
            }
        }
        if with_user {
            self.filters.iter().for_each(|f| push(*f));
        }
        (left, right)
    }

    /// Correction layers only, as left and right filter lists.
    pub fn correction_chain(&self) -> (Vec<Filter>, Vec<Filter>) {
        self.split(false)
    }

    pub fn chain(&self, rate: f64) -> Chain {
        let (left, right) = self.split(true);
        let peak = if left == right {
            curve::peak_db(&left, rate)
        } else {
            curve::peak_db(&left, rate).max(curve::peak_db(&right, rate))
        };
        let preamp_db = if self.preamp.auto {
            curve::auto_preamp(peak)
        } else if self.preamp.db.is_finite() {
            self.preamp.db
        } else {
            0.0
        };
        Chain { left, right, preamp_db, peak_db: peak }
    }

    /// Reads any known schema version and upgrades it to the current one.
    pub fn from_json(text: &str) -> Result<Profile, String> {
        let mut value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("This file is not a valid profile: {e}"))?;
        let schema = value.get("schema").and_then(|v| v.as_u64()).unwrap_or(0);
        if schema > u64::from(SCHEMA) {
            return Err("This profile was made by a newer version of EQaura.".into());
        }
        if let Some(obj) = value.as_object_mut() {
            // Schema 0: the plain layout from early notes, with a flat preamp_db.
            if schema == 0
                && let Some(db) = obj.remove("preamp_db").and_then(|v| v.as_f64())
            {
                obj.insert("preamp".into(), serde_json::json!({ "auto": false, "db": db }));
            }
            obj.insert("schema".into(), SCHEMA.into());
            obj.entry("name").or_insert_with(|| "Untitled".into());
        }
        let profile: Profile =
            serde_json::from_value(value).map_err(|e| format!("This profile could not be read: {e}"))?;
        Ok(profile)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}
