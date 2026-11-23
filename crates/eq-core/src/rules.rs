//! Plain-English tuning that works with no key and no internet.

use crate::change::{Change, nudge};
use crate::filter::{Filter, Kind};

#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub reason: String,
    pub changes: Vec<Change>,
}

/// (kind, frequency, gain, q) for "more of this".
type Move = (Kind, f64, f64, f64);

use Kind::{HighPass, HighShelf as HS, LowShelf as LS, Peaking as PK};

/// A sound quality that can be turned up or down.
struct Quality {
    words: &'static [&'static str],
    label: &'static str,
    more: &'static [Move],
    less: &'static [Move],
    /// Complaints such as "harsh" already mean "reduce this" on their own.
    complaint: bool,
}

const QUALITIES: &[Quality] = &[
    Quality { words: &["sub bass", "sub-bass", "subbass", "rumble", "deep bass"], label: "sub-bass",
        more: &[(LS, 45.0, 4.0, 0.8)], less: &[(LS, 45.0, -4.0, 0.8)], complaint: false },
    Quality { words: &["boomy", "booming", "bloated"], label: "boominess",
        more: &[(PK, 120.0, 3.0, 1.0)], less: &[(PK, 120.0, -3.5, 1.0)], complaint: true },
    Quality { words: &["muddy", "mud", "muffled", "murky", "unclear"], label: "muddiness",
        more: &[(PK, 280.0, 3.0, 1.0)], less: &[(PK, 280.0, -3.0, 1.0), (PK, 3000.0, 1.5, 0.9)], complaint: true },
    Quality { words: &["boxy", "box"], label: "boxiness",
        more: &[(PK, 450.0, 2.5, 1.2)], less: &[(PK, 450.0, -3.0, 1.2)], complaint: true },
    Quality { words: &["nasal", "honky", "honk"], label: "nasal sound",
        more: &[(PK, 1000.0, 2.5, 1.4)], less: &[(PK, 1000.0, -3.0, 1.4)], complaint: true },
    Quality { words: &["sibilant", "sibilance", "hissy", "lispy", "ess sounds", "s sounds"], label: "sibilance",
        more: &[(PK, 7000.0, 2.0, 2.5)], less: &[(PK, 7000.0, -4.0, 2.5)], complaint: true },
    Quality { words: &["harsh", "harshness", "piercing", "shouty", "fatiguing", "tiring", "sharp", "shrill", "screechy", "strident"], label: "harshness",
        more: &[(PK, 3500.0, 2.5, 1.2)], less: &[(PK, 3500.0, -3.0, 1.2), (PK, 8000.0, -1.5, 1.5)], complaint: true },
    Quality { words: &["tinny", "metallic"], label: "tinny sound",
        more: &[(PK, 5000.0, 2.0, 1.0)], less: &[(PK, 5000.0, -2.5, 1.0), (LS, 120.0, 2.5, 0.7)], complaint: true },
    Quality { words: &["thin", "hollow", "weak", "no body", "lifeless"], label: "thinness",
        more: &[(LS, 150.0, -2.0, 0.7)], less: &[(LS, 110.0, 3.0, 0.7), (PK, 250.0, 1.5, 0.9)], complaint: true },
    Quality { words: &["dull", "dark", "veiled", "flat sounding", "lacks detail"], label: "dullness",
        more: &[(HS, 6000.0, -2.5, 0.7)], less: &[(HS, 6000.0, 3.0, 0.7), (PK, 3000.0, 1.0, 0.9)], complaint: true },
    Quality { words: &["vocals far", "voice far", "vocal far", "distant", "far away", "recessed", "buried"], label: "distance",
        more: &[(PK, 2000.0, -2.5, 0.9)], less: &[(PK, 2000.0, 3.0, 0.9), (PK, 300.0, -1.5, 1.0)], complaint: true },
    Quality { words: &["punch", "punchy", "kick", "impact", "slam", "thump"], label: "punch",
        more: &[(PK, 90.0, 3.5, 1.2)], less: &[(PK, 90.0, -3.5, 1.2)], complaint: false },
    Quality { words: &["bass", "low end", "lows", "bassy", "low frequencies"], label: "bass",
        more: &[(LS, 105.0, 4.0, 0.7)], less: &[(LS, 105.0, -4.0, 0.7)], complaint: false },
    Quality { words: &["warm", "warmer", "warmth", "body", "fuller", "full", "richer", "rich", "cozy"], label: "warmth",
        more: &[(PK, 200.0, 2.5, 0.8), (HS, 8000.0, -1.0, 0.7)], less: &[(PK, 200.0, -2.5, 0.8)], complaint: false },
    Quality { words: &["vocal", "vocals", "voice", "voices", "singer", "singing"], label: "vocals",
        more: &[(PK, 2200.0, 3.0, 0.9)], less: &[(PK, 2200.0, -3.0, 0.9)], complaint: false },
    Quality { words: &["dialogue", "dialog", "speech", "podcast", "podcasts", "audiobook", "call", "calls", "talking"], label: "speech clarity",
        more: &[(PK, 2500.0, 3.5, 0.9), (PK, 200.0, -2.5, 0.8)], less: &[(PK, 2500.0, -3.0, 0.9)], complaint: false },
    Quality { words: &["mids", "midrange", "mid", "middle"], label: "mids",
        more: &[(PK, 1000.0, 3.0, 0.7)], less: &[(PK, 1000.0, -3.0, 0.7)], complaint: false },
    Quality { words: &["footstep", "footsteps", "gaming", "fps", "game"], label: "game detail",
        more: &[(PK, 3000.0, 4.0, 0.9), (LS, 100.0, -3.0, 0.7)], less: &[(PK, 3000.0, -3.0, 0.9)], complaint: false },
    Quality { words: &["guitar", "guitars"], label: "guitars",
        more: &[(PK, 2800.0, 2.5, 1.0)], less: &[(PK, 2800.0, -2.5, 1.0)], complaint: false },
    Quality { words: &["piano", "keys"], label: "piano",
        more: &[(PK, 1500.0, 2.0, 0.8)], less: &[(PK, 1500.0, -2.0, 0.8)], complaint: false },
    Quality { words: &["drum", "drums", "snare"], label: "drums",
        more: &[(PK, 90.0, 2.5, 1.2), (PK, 4500.0, 2.0, 1.2)], less: &[(PK, 90.0, -2.5, 1.2), (PK, 4500.0, -2.0, 1.2)], complaint: false },
    Quality { words: &["cymbal", "cymbals", "hi hat", "hi-hat", "hats"], label: "cymbals",
        more: &[(PK, 9000.0, 2.5, 1.2)], less: &[(PK, 9000.0, -3.0, 1.2)], complaint: false },
    Quality { words: &["clarity", "clear", "clearer", "detail", "detailed", "definition", "crisp", "crisper"], label: "clarity",
        more: &[(PK, 3200.0, 2.5, 0.9), (PK, 280.0, -1.5, 1.0)], less: &[(PK, 3200.0, -2.5, 0.9)], complaint: false },
    Quality { words: &["presence", "forward", "closer", "upfront"], label: "presence",
        more: &[(PK, 4000.0, 2.5, 1.0)], less: &[(PK, 4000.0, -2.5, 1.0)], complaint: false },
    Quality { words: &["air", "airy", "sparkle", "sparkly", "open", "space", "spacious"], label: "air",
        more: &[(HS, 11000.0, 3.5, 0.7)], less: &[(HS, 11000.0, -3.5, 0.7)], complaint: false },
    Quality { words: &["treble", "high end", "highs", "bright", "brighter", "brightness"], label: "treble",
        more: &[(HS, 7000.0, 3.0, 0.7)], less: &[(HS, 7000.0, -3.0, 0.7)], complaint: false },
    Quality { words: &["smooth", "smoother", "relaxed", "softer", "gentle", "mellow"], label: "smoothness",
        more: &[(PK, 3500.0, -2.0, 1.0), (HS, 8000.0, -2.0, 0.7)], less: &[(PK, 3500.0, 2.0, 1.0)], complaint: false },
];

const LESS: &[&str] = &["less", "reduce", "lower", "cut", "decrease", "remove", "tame", "fewer", "turn down", "too much", "too", "drop", "without", "get rid", "fix", "quieter", "soften"];
const MORE: &[&str] = &["more", "boost", "add", "increase", "raise", "louder", "turn up", "bring out", "need", "want", "lacks", "lack", "missing", "not enough", "stronger", "bring up"];
const NEGATED: &[&str] = &["not", "no", "don't", "dont", "isn't", "isnt", "never"];
const SOFT: &[&str] = &["a bit", "a little", "slightly", "bit", "touch", "little", "tad", "somewhat"];
const HARD: &[&str] = &["much", "a lot", "lot", "way", "very", "really", "extremely", "far more", "loads"];
/// Words that cannot change direction, so they do not separate two requests.
const JOINERS: &[&str] = &["and", "but", "then", "also", "while", "with", "plus"];

fn clean(text: &str) -> String {
    let mapped: String = text
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '\'' | '-' | ' ' => c,
            '\u{2019}' => '\'',
            ',' | ';' | '.' | '&' => ';',
            _ => ' ',
        })
        .collect();
    mapped.replace(';', " ; ")
}

/// Splits into single requests: "more bass, less harsh" becomes two.
fn clauses(text: &str) -> Vec<String> {
    let mut out = vec![Vec::new()];
    for word in text.split_whitespace() {
        let turns = matches!(word, "more" | "less")
            && out.last().is_some_and(|c: &Vec<&str>| c.iter().any(|w| matches!(*w, "more" | "less")));
        if turns {
            out.push(vec![word]);
        } else if word == ";" || JOINERS.contains(&word) {
            out.push(Vec::new());
        } else if let Some(last) = out.last_mut() {
            last.push(word);
        }
    }
    out.into_iter().filter(|c| !c.is_empty()).map(|c| c.join(" ")).collect()
}

fn has(text: &str, phrase: &str) -> bool {
    format!(" {text} ").contains(&format!(" {phrase} "))
}

fn any(text: &str, list: &[&str]) -> bool {
    list.iter().any(|p| has(text, p))
}

/// Suggests changes for a request, or `None` when nothing was understood.
pub fn suggest(request: &str, filters: &[Filter]) -> Option<Suggestion> {
    let text = clean(request);
    let flat = text.replace(" ; ", " ");
    if any(&flat, &["reset", "start over", "make it flat", "flat", "remove all", "clear everything", "remove everything"])
        && !has(&flat, "flat sounding")
    {
        return (!filters.is_empty())
            .then(|| Suggestion { reason: "Back to a flat curve.".into(), changes: vec![Change::Clear] });
    }
    if any(&flat, &["night", "quiet listening", "late night", "low volume", "quiet volume"]) {
        let moves: &[Move] = if any(&flat, &["low volume", "quiet volume"]) {
            &[(LS, 105.0, 4.0, 0.7), (HS, 9000.0, 2.5, 0.7)]
        } else {
            &[(LS, 80.0, -4.0, 0.7), (PK, 2500.0, 1.5, 0.8), (HS, 9000.0, -2.0, 0.7)]
        };
        return Some(Suggestion {
            reason: if any(&flat, &["low volume", "quiet volume"]) {
                "Fills in the bass and treble that fade at low volume.".into()
            } else {
                "Softer bass and treble with clear voices, for late listening.".into()
            },
            changes: merge(filters, moves.iter().map(|m| (*m, 1.0))),
        });
    }
    if any(&flat, &["v shape", "v-shape", "v shaped", "v-shaped", "fun", "exciting"]) {
        let moves: &[Move] = &[(LS, 100.0, 4.0, 0.7), (PK, 800.0, -1.5, 0.7), (HS, 8000.0, 3.0, 0.7)];
        return Some(Suggestion {
            reason: "Raised bass and treble for an energetic sound.".into(),
            changes: merge(filters, moves.iter().map(|m| (*m, 1.0))),
        });
    }
    if any(&flat, &["handling noise", "wind noise", "remove rumble", "rumble filter"]) {
        return Some(Suggestion {
            reason: "Removes everything below the music.".into(),
            changes: vec![Change::Add(Filter::new(HighPass, 30.0, 0.0, 0.71))],
        });
    }

    let mut moves: Vec<(Move, f64)> = Vec::new();
    let mut said: Vec<String> = Vec::new();
    let mut direction: Option<bool> = None;
    let mut strength: Option<f64> = None;
    for clause in clauses(&text) {
        let asks_less = any(&clause, LESS);
        let asks_more = any(&clause, MORE);
        let not_enough = any(&clause, &["not enough", "lacks", "lack", "missing"]);
        let mut rest = clause.clone();
        let mut found = false;
        // In "the vocals are harsh", vocals only says where the problem is.
        let has_complaint = QUALITIES.iter().any(|q| q.complaint && any(&clause, q.words));
        let only_subject = has_complaint && !any(&clause, &["more", "boost", "add", "increase", "raise", "louder"]);
        for q in QUALITIES {
            if only_subject && !q.complaint {
                found |= any(&clause, q.words);
                continue;
            }
            // Longest phrase first, so "sub bass" is not read as "bass" too.
            let mut words: Vec<&&str> = q.words.iter().filter(|w| has(&rest, w)).collect();
            words.sort_by_key(|w| std::cmp::Reverse(w.len()));
            let Some(word) = words.first() else { continue };
            found = true;
            let negated = any(&clause, NEGATED) && !not_enough;
            let up = if q.complaint {
                // "sounds harsh" and "less harsh" both mean reduce it.
                asks_more && !asks_less && !negated && !any(&clause, &["need", "want"])
            } else if not_enough {
                true
            } else if asks_less || negated {
                false
            } else if asks_more {
                true
            } else {
                direction.unwrap_or(true)
            };
            if !q.complaint {
                direction = Some(up);
            }
            let power = if any(&clause, SOFT) {
                0.5
            } else if any(&clause, HARD) {
                1.5
            } else {
                strength.unwrap_or(1.0)
            };
            strength = Some(power);
            moves.extend((if up { q.more } else { q.less }).iter().map(|m| (*m, power)));
            said.push(format!("{} {}", if up { "more" } else { "less" }, q.label));
            rest = format!(" {rest} ").replace(&format!(" {word} "), " ; ").trim().to_owned();
        }
        if !found {
            // "reduce" alone can apply to the next clause: "lower, please, the bass".
            if asks_less {
                direction = Some(false);
            } else if asks_more {
                direction = Some(true);
            }
        }
    }
    if moves.is_empty() {
        return None;
    }
    let changes = merge(filters, moves.into_iter());
    if changes.is_empty() {
        return None;
    }
    said.dedup();
    let mut reason = said.join(", ");
    if let Some(c) = reason.get_mut(0..1) {
        c.make_ascii_uppercase();
    }
    Some(Suggestion { reason: reason + ".", changes })
}

/// Folds moves into existing bands, and into each other.
fn merge(filters: &[Filter], moves: impl Iterator<Item = (Move, f64)>) -> Vec<Change> {
    let mut combined: Vec<Move> = Vec::new();
    for ((kind, freq, gain, q), power) in moves {
        let gain = gain * power;
        match combined
            .iter_mut()
            .find(|m| m.0 == kind && (m.1 / freq).log2().abs() < 0.2 && (m.3 / q).log2().abs() < 0.6)
        {
            Some(m) => m.2 += gain,
            None => combined.push((kind, freq, gain, q)),
        }
    }
    let mut out: Vec<Change> = Vec::new();
    for (kind, freq, gain, q) in combined {
        let gain = (gain * 10.0).round() / 10.0;
        if gain == 0.0 {
            continue;
        }
        let change = nudge(filters, kind, freq, gain, q);
        let taken = |i: usize| out.iter().any(|c| matches!(c, Change::Update(j, _) | Change::Remove(j) if *j == i));
        match change {
            // Two moves must not fight over one existing band.
            Change::Update(i, _) | Change::Remove(i) if taken(i) => {
                out.push(Change::Add(Filter::new(kind, freq, gain, q)));
            }
            c => out.push(c),
        }
    }
    out
}
