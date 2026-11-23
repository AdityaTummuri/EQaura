//! Edits proposed by the assistant or the built-in rules.

use crate::filter::{Channel, Filter, Kind};
use crate::profile::Profile;
use crate::text;

#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Add(Filter),
    /// Replaces the band at this index.
    Update(usize, Filter),
    Remove(usize),
    /// Removes every user band.
    Clear,
}

/// Adds a gain change, merging into a matching band instead of stacking a new one.
pub fn nudge(filters: &[Filter], kind: Kind, freq: f64, gain: f64, q: f64) -> Change {
    let near = filters.iter().position(|f| {
        f.kind == kind
            && f.on
            && f.channel == Channel::Both
            && (f.freq_hz / freq).log2().abs() < 0.2
            && (f.q / q).log2().abs() < 0.6
    });
    match near {
        Some(i) => {
            let gain_db = ((filters[i].gain_db + gain) * 10.0).round() / 10.0;
            if gain_db == 0.0 {
                Change::Remove(i)
            } else {
                Change::Update(i, Filter { gain_db, ..filters[i] })
            }
        }
        None => Change::Add(Filter::new(kind, freq, gain, q)),
    }
}

/// Applies all changes against the original band numbers, or none of them.
pub fn apply_changes(profile: &mut Profile, changes: &[Change]) -> Result<(), String> {
    let n = profile.filters.len();
    let mut slots: Vec<Option<Filter>> = profile.filters.iter().copied().map(Some).collect();
    let mut added = Vec::new();
    for c in changes {
        match c {
            Change::Add(f) if f.is_valid() => added.push(*f),
            Change::Update(i, f) if f.is_valid() && *i < n => slots[*i] = Some(*f),
            Change::Remove(i) if *i < n => slots[*i] = None,
            Change::Clear => slots.iter_mut().for_each(|s| *s = None),
            Change::Update(i, _) | Change::Remove(i) if *i >= n => {
                return Err(format!("There is no band {}.", i + 1));
            }
            _ => return Err("One of the changes has numbers that do not make a working filter.".into()),
        }
    }
    profile.filters = slots.into_iter().flatten().chain(added).collect();
    Ok(())
}

fn describe_filter(f: &Filter) -> String {
    let mut s = format!("{} at {}", f.kind.label(), text::freq(f.freq_hz));
    if f.kind.has_gain() {
        s += &format!(", {}", text::gain(f.gain_db));
    }
    s += &format!(", Q {}", text::q(f.q));
    match f.channel {
        Channel::Both => {}
        Channel::Left => s += ", left ear",
        Channel::Right => s += ", right ear",
    }
    s
}

pub fn describe_change(filters: &[Filter], c: &Change) -> String {
    match c {
        Change::Add(f) => format!("Add {}", lower_first(&describe_filter(f))),
        Change::Update(i, f) => match filters.get(*i) {
            Some(old)
                if old.kind == f.kind
                    && old.freq_hz == f.freq_hz
                    && old.q == f.q
                    && old.channel == f.channel
                    && old.on == f.on =>
            {
                format!(
                    "Band {} at {}: {} to {}",
                    i + 1,
                    text::freq(f.freq_hz),
                    text::gain(old.gain_db),
                    text::gain(f.gain_db)
                )
            }
            _ => format!(
                "Band {}: {}{}",
                i + 1,
                lower_first(&describe_filter(f)),
                if f.on { "" } else { ", off" }
            ),
        },
        Change::Remove(i) => match filters.get(*i) {
            Some(f) => format!("Remove band {} ({})", i + 1, lower_first(&describe_filter(f))),
            None => format!("Remove band {}", i + 1),
        },
        Change::Clear => "Remove all bands".into(),
    }
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}
