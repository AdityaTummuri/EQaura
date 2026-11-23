//! Advice about a profile. Nothing here blocks or changes a setting.

use crate::filter::Filter;
use crate::profile::Profile;
use crate::text;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Caution,
    Danger,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Warning {
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    /// Index of the user band this is about.
    pub band: Option<usize>,
}

fn w(severity: Severity, title: String, detail: &str, band: Option<usize>) -> Warning {
    Warning { severity, title, detail: detail.into(), band }
}

/// All advice for a profile, most important first.
pub fn check(profile: &Profile, rate: f64) -> Vec<Warning> {
    let chain = profile.chain(rate);
    let mut out = Vec::new();
    let net = chain.net_peak_db();
    if net > 0.05 {
        out.push(w(
            if net > 6.0 { Severity::Danger } else { Severity::Caution },
            format!("Loud parts can distort by up to {}", text::gain(net)),
            "Turn on Auto preamp, or lower the preamp, to leave room for the boosts.",
            None,
        ));
    }
    if net > 6.0 {
        out.push(w(
            Severity::Danger,
            "This is much louder than normal".into(),
            "Turn the volume down before you listen. Loud sound can damage hearing.",
            None,
        ));
    }
    if chain.preamp_db < -18.0 {
        out.push(w(
            Severity::Info,
            format!("Everything gets {} quieter", text::gain(chain.preamp_db).replace('\u{2212}', "")),
            "Large boosts need a low preamp. Smaller boosts keep more volume.",
            None,
        ));
    }
    for (i, f) in profile.filters.iter().enumerate() {
        out.extend(check_band(i, f));
    }
    let on = profile.filters.iter().filter(|f| f.is_active()).count();
    if on > 20 {
        out.push(w(
            Severity::Info,
            format!("{on} bands are active"),
            "More than 20 bands uses more processing and is hard to tune by ear.",
            None,
        ));
    }
    if let Some(h) = profile.hearing.as_ref().filter(|h| h.on) {
        let top = h
            .left
            .iter()
            .chain(&h.right)
            .map(|f| f.gain_db * h.strength)
            .fold(0.0, f64::max);
        if top > 10.0 {
            out.push(w(
                Severity::Caution,
                format!("Hearing compensation boosts by up to {}", text::gain(top)),
                "Lower its strength if music sounds sharp or tiring. This is not a hearing aid.",
                None,
            ));
        }
    }
    out.sort_by_key(|w| std::cmp::Reverse(w.severity));
    out
}

fn check_band(i: usize, f: &Filter) -> Vec<Warning> {
    let mut out = Vec::new();
    if !f.on {
        return out;
    }
    let n = i + 1;
    if !f.is_valid() {
        out.push(w(
            Severity::Caution,
            format!("Band {n} is ignored"),
            "Its numbers do not make a working filter.",
            Some(i),
        ));
        return out;
    }
    if f.kind.has_gain() && f.gain_db > 12.0 {
        out.push(w(
            if f.gain_db > 24.0 { Severity::Danger } else { Severity::Caution },
            format!("Band {n} boosts by {}", text::gain(f.gain_db)),
            "Boosts above +12 dB often distort and can be unpleasant.",
            Some(i),
        ));
    }
    if f.kind.has_gain() && f.gain_db < -24.0 {
        out.push(w(
            Severity::Info,
            format!("Band {n} cuts by {}", text::gain(f.gain_db).replace('\u{2212}', "")),
            "Cuts this deep remove that part of the sound almost completely.",
            Some(i),
        ));
    }
    if f.q > 10.0 {
        out.push(w(
            Severity::Caution,
            format!("Band {n} is very narrow (Q {})", text::q(f.q)),
            "Very narrow bands can ring after loud sounds.",
            Some(i),
        ));
    }
    if f.q < 0.1 {
        out.push(w(
            Severity::Info,
            format!("Band {n} is very wide (Q {})", text::q(f.q)),
            "It changes almost the whole range, much like the volume control.",
            Some(i),
        ));
    }
    if !(20.0..=20_000.0).contains(&f.freq_hz) {
        out.push(w(
            Severity::Info,
            format!("Band {n} is centred at {}", text::freq(f.freq_hz)),
            "Most people hear 20 Hz to 20 kHz. Strong boosts below that can strain speakers.",
            Some(i),
        ));
    }
    out
}
