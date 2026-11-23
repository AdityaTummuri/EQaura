//! Headphone corrections from the AutoEq project, packed into the app.

use crate::filter::{Filter, Kind};
use std::sync::OnceLock;

static PACKED: &[u8] = include_bytes!("../data/autoeq.bin");

/// Measurement sources, best first. Used only to order equal matches.
const SOURCE_ORDER: [&str; 8] = [
    "oratory1990",
    "crinacle",
    "Rtings",
    "Headphone.com Legacy",
    "Innerfidelity",
    "Super Review",
    "HypetheSonics",
    "Kazi",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: &'static str,
    pub source: &'static str,
    pub form: &'static str,
    filters: &'static str,
}

struct Db {
    entries: Vec<Entry>,
    lower: Vec<String>,
}

fn db() -> &'static Db {
    static DB: OnceLock<Db> = OnceLock::new();
    DB.get_or_init(|| {
        let text = miniz_oxide::inflate::decompress_to_vec_zlib(PACKED)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_default();
        let text: &'static str = Box::leak(text.into_boxed_str());
        let mut entries = Vec::with_capacity(9000);
        for line in text.lines() {
            let mut p = line.split('\t');
            if let (Some(name), Some(source), Some(form), Some(filters)) =
                (p.next(), p.next(), p.next(), p.next())
            {
                entries.push(Entry { name, source, form, filters });
            }
        }
        let lower = entries.iter().map(|e| normalize(e.name)).collect();
        Db { entries, lower }
    })
}

fn normalize(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| if c.is_alphanumeric() || c == '+' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The rest of the name from the start of each word.
fn word_starts(name: &str) -> impl Iterator<Item = &str> {
    std::iter::once(name).chain(name.match_indices(' ').map(move |(i, _)| &name[i + 1..]))
}

pub fn len() -> usize {
    db().entries.len()
}

pub fn get(i: usize) -> Option<Entry> {
    db().entries.get(i).copied()
}

/// Indices of matching headphones, best match first. Every word must match;
/// "hd600" also finds "HD 600".
pub fn search(query: &str) -> Vec<usize> {
    let db = db();
    let q = normalize(query);
    if q.is_empty() {
        return (0..db.entries.len()).collect();
    }
    let words: Vec<&str> = q.split(' ').collect();
    let squashed_q = q.replace(' ', "");
    let mut hits: Vec<(u32, usize, usize, usize)> = Vec::new();
    for (i, name) in db.lower.iter().enumerate() {
        let squashed = name.replace(' ', "");
        let score = if *name == q || squashed == squashed_q {
            0
        } else if name.starts_with(&q) {
            1
        } else if words.iter().all(|w| name.split(' ').any(|n| n.starts_with(w)))
            || word_starts(name).any(|s| s.replace(' ', "").starts_with(&squashed_q))
        {
            2
        } else if squashed.contains(&squashed_q) {
            3
        } else if words.iter().all(|w| squashed.contains(w)) {
            4
        } else {
            continue;
        };
        let src = db.entries[i].source;
        let rank = SOURCE_ORDER.iter().position(|s| src.starts_with(s)).unwrap_or(SOURCE_ORDER.len());
        hits.push((score, name.len(), rank, i));
    }
    hits.sort_unstable();
    hits.into_iter().map(|h| h.3).collect()
}

impl Entry {
    pub fn filters(&self) -> Vec<Filter> {
        self.filters
            .split(';')
            .filter_map(|part| {
                let mut p = part.split(',');
                let kind = match p.next()? {
                    "P" => Kind::Peaking,
                    "L" => Kind::LowShelf,
                    "H" => Kind::HighShelf,
                    _ => return None,
                };
                let f = p.next()?.parse().ok()?;
                let g = p.next()?.parse().ok()?;
                let q = p.next()?.parse().ok()?;
                Some(Filter::new(kind, f, g, q)).filter(Filter::is_valid)
            })
            .collect()
    }
}
