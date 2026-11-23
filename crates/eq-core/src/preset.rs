//! Equalizer APO / AutoEq parametric text format.

use crate::filter::{Channel, Filter, Kind};
use crate::profile::{Preamp, Profile};
use std::fmt::Write;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parsed {
    pub preamp_db: Option<f64>,
    pub filters: Vec<Filter>,
    /// Lines that were understood as settings but could not be used.
    pub skipped: Vec<String>,
}

impl Parsed {
    /// Turns the file into a profile. A preamp that only undoes the boosts
    /// becomes the automatic one, so later edits stay safe.
    pub fn into_profile(self, name: &str) -> Profile {
        let mut p = Profile { name: name.into(), filters: self.filters, ..Profile::default() };
        if let Some(db) = self.preamp_db {
            let auto = p.chain(48_000.0).preamp_db;
            p.preamp = Preamp { auto: (db - auto).abs() < 0.35, db };
        }
        p
    }
}

pub fn parse(input: &str) -> Parsed {
    let mut out = Parsed::default();
    let mut channel = Some(Channel::Both);
    for raw in input.trim_start_matches('\u{feff}').lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, rest)) = line.split_once(':') else { continue };
        let key = key.trim().to_ascii_lowercase();
        if key == "preamp" {
            match number_then_unit(rest, "db") {
                Some(v) => out.preamp_db = Some(out.preamp_db.unwrap_or(0.0) + v),
                None => out.skipped.push(line.into()),
            }
        } else if key == "channel" {
            channel = parse_channel(rest);
            if channel.is_none() {
                out.skipped.push(line.into());
            }
        } else if key == "filter" || key.starts_with("filter ") {
            match (parse_filter(rest), channel) {
                (Line::Filter(mut f), Some(ch)) => {
                    f.channel = ch;
                    out.filters.push(f);
                }
                (Line::Empty, _) => {}
                _ => out.skipped.push(line.into()),
            }
        } else if matches!(key.as_str(), "graphiceq" | "convolution" | "delay" | "copy" | "include") {
            out.skipped.push(line.into());
        }
    }
    out
}

fn parse_channel(rest: &str) -> Option<Channel> {
    let names: Vec<String> = rest.split_whitespace().map(|s| s.to_ascii_uppercase()).collect();
    let has = |n: &str| names.iter().any(|x| x == n);
    let left = has("L") || has("1");
    let right = has("R") || has("2");
    match (has("ALL") || (left && right), left, right) {
        (true, ..) => Some(Channel::Both),
        (_, true, _) => Some(Channel::Left),
        (_, _, true) => Some(Channel::Right),
        _ => None,
    }
}

fn num(s: &str) -> Option<f64> {
    let s = s.replace('\u{2212}', "-");
    let s = if s.contains('.') { s.replace(',', "") } else { s.replace(',', ".") };
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn number_then_unit(rest: &str, unit: &str) -> Option<f64> {
    let t: Vec<&str> = rest.split_whitespace().collect();
    let first = t.first()?;
    if let Some(v) = num(first) {
        return Some(v);
    }
    let lower = first.to_ascii_lowercase();
    num(lower.strip_suffix(unit)?)
}

enum Line {
    Filter(Filter),
    Empty,
    Bad,
}

fn parse_filter(rest: &str) -> Line {
    // AutoEq and REW pad with spaces; "Hz" and "dB" may touch their number.
    let lower = rest.to_ascii_lowercase();
    let t: Vec<&str> = lower.split_whitespace().collect();
    let on = match t.first() {
        Some(&"on") => true,
        Some(&"off") => false,
        _ => return Line::Bad,
    };
    let Some(kind_word) = t.get(1) else { return Line::Bad };
    let (kind, corner) = match *kind_word {
        "none" => return Line::Empty,
        "pk" | "peq" | "modal" => (Kind::Peaking, false),
        "lsc" | "lsq" => (Kind::LowShelf, false),
        "hsc" | "hsq" => (Kind::HighShelf, false),
        "ls" => (Kind::LowShelf, true),
        "hs" => (Kind::HighShelf, true),
        "lp" | "lpq" => (Kind::LowPass, false),
        "hp" | "hpq" => (Kind::HighPass, false),
        "no" => (Kind::Notch, false),
        _ => return Line::Bad,
    };
    let value_after = |word: &str| -> Option<f64> {
        let i = t.iter().position(|x| *x == word)?;
        let s = t.get(i + 1)?;
        num(s).or_else(|| num(s.trim_end_matches("hz").trim_end_matches("db")))
    };
    let Some(mut freq) = value_after("fc") else { return Line::Bad };
    let khz = t.iter().position(|x| *x == "fc").and_then(|i| t.get(i + 2)) == Some(&"khz");
    if khz {
        freq *= 1000.0;
    }
    let gain = if kind.has_gain() { value_after("gain").unwrap_or(0.0) } else { 0.0 };
    // "LS 6 dB Fc ..." gives the slope before the frequency.
    let slope = t.get(2).filter(|w| **w != "fc").and_then(|w| num(w.trim_end_matches("db")));
    let shelf = matches!(kind, Kind::LowShelf | Kind::HighShelf);
    let q = if let Some(q) = value_after("q") {
        q
    } else if let Some(i) = t.iter().position(|x| *x == "bw") {
        // "BW Oct 1.0"
        let octaves = match t.get(i + 1) {
            Some(&"oct") => t.get(i + 2).and_then(|s| num(s)),
            Some(s) => num(s),
            None => None,
        };
        let Some(oct) = octaves.filter(|o| *o > 0.0) else { return Line::Bad };
        let p = 2f64.powf(oct);
        p.sqrt() / (p - 1.0)
    } else if shelf {
        let s = slope.map_or(0.9, |db| db / 12.0);
        if s <= 0.0 {
            return Line::Bad;
        }
        let a = 10f64.powf(gain.abs() / 40.0);
        let inv = ((a + 1.0 / a) * (1.0 / s - 1.0) + 2.0).max(1e-4).sqrt();
        if corner {
            let factor = 10f64.powf(gain.abs() / 80.0 / s);
            if kind == Kind::LowShelf { freq *= factor } else { freq /= factor }
        }
        1.0 / inv
    } else if kind == Kind::Notch {
        30.0
    } else {
        std::f64::consts::FRAC_1_SQRT_2
    };
    let f = Filter { kind, freq_hz: freq, gain_db: gain, q, on, channel: Channel::Both };
    if f.is_valid() { Line::Filter(f) } else { Line::Bad }
}

fn kind_token(k: Kind) -> &'static str {
    match k {
        Kind::Peaking => "PK",
        Kind::LowShelf => "LSC",
        Kind::HighShelf => "HSC",
        Kind::LowPass => "LPQ",
        Kind::HighPass => "HPQ",
        Kind::Notch => "NO",
    }
}

fn short(v: f64, places: usize) -> String {
    let s = format!("{v:.places$}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.into() }
}

/// One `Filter` line without its channel.
pub fn filter_line(n: usize, f: &Filter) -> String {
    let mut s = format!(
        "Filter {n}: {} {} Fc {} Hz",
        if f.on { "ON" } else { "OFF" },
        kind_token(f.kind),
        // Enough digits that tiny values stay valid when read back.
        short(f.freq_hz, if f.freq_hz < 1.0 { 12 } else { 3 }),
    );
    if f.kind.has_gain() {
        let _ = write!(s, " Gain {} dB", short(f.gain_db, 3));
    }
    let _ = write!(s, " Q {}", short(f.q, if f.q < 0.01 { 12 } else { 4 }));
    s
}

/// Writes filters and preamp; per-ear filters go under `Channel:` lines.
pub fn write(preamp_db: f64, filters: &[Filter]) -> String {
    let mut s = format!("Preamp: {} dB\n", short(preamp_db, 2));
    let mut n = 0;
    for (ch, head) in [(Channel::Both, ""), (Channel::Left, "Channel: L\n"), (Channel::Right, "Channel: R\n")] {
        let mut first = true;
        for f in filters.iter().filter(|f| f.channel == ch && f.is_valid()) {
            if first {
                s.push_str(head);
                first = false;
            }
            n += 1;
            s.push_str(&filter_line(n, f));
            s.push('\n');
        }
    }
    if filters.iter().any(|f| f.channel != Channel::Both) {
        s.push_str("Channel: all\n");
    }
    s
}

/// Exports everything that is audible, including correction layers.
pub fn export(profile: &Profile) -> String {
    let chain = profile.chain(48_000.0);
    let mut filters: Vec<Filter>;
    if chain.is_symmetric() {
        filters = chain.left.clone();
    } else {
        filters = Vec::new();
        for f in &chain.left {
            filters.push(Filter { channel: Channel::Left, ..*f });
        }
        for f in &chain.right {
            filters.push(Filter { channel: Channel::Right, ..*f });
        }
    }
    format!("# {}\n# Made with EQaura\n{}", profile.name.replace(['\n', '\r'], " "), write(chain.preamp_db, &filters))
}
