use eq_core::change::Change;
use eq_core::filter::{Channel, Filter, Kind};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub reason: String,
    pub changes: Vec<Change>,
}

const TYPES: [(&str, Kind); 6] = [
    ("peaking", Kind::Peaking),
    ("low_shelf", Kind::LowShelf),
    ("high_shelf", Kind::HighShelf),
    ("low_pass", Kind::LowPass),
    ("high_pass", Kind::HighPass),
    ("notch", Kind::Notch),
];

pub fn type_name(kind: Kind) -> &'static str {
    TYPES.iter().find(|(_, k)| *k == kind).map_or("peaking", |(n, _)| n)
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["reason", "changes"],
        "properties": {
            "reason": { "type": "string" },
            "changes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["action", "band", "type", "freq_hz", "gain_db", "q", "ear"],
                    "properties": {
                        "action": { "type": "string", "enum": ["add", "update", "remove", "clear"] },
                        "band": { "type": "integer" },
                        "type": { "type": "string", "enum": TYPES.map(|t| t.0) },
                        "freq_hz": { "type": "number" },
                        "gain_db": { "type": "number" },
                        "q": { "type": "number" },
                        "ear": { "type": "string", "enum": ["both", "left", "right"] }
                    }
                }
            }
        }
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Add,
    Update,
    Remove,
    Clear,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChange {
    action: Action,
    #[serde(default)]
    band: i64,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    freq_hz: Option<f64>,
    #[serde(default)]
    gain_db: Option<f64>,
    #[serde(default)]
    q: Option<f64>,
    #[serde(default)]
    ear: Option<Channel>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAnswer {
    #[serde(default)]
    reason: String,
    changes: Vec<RawChange>,
}

/// The first complete JSON object in the text. Models sometimes wrap it in
/// prose or a code fence.
fn json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for (i, b) in text.bytes().enumerate().skip(start) {
        match b {
            _ if escaped => escaped = false,
            b'\\' if in_string => escaped = true,
            b'"' => in_string = !in_string,
            b'{' if !in_string => depth += 1,
            b'}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return text.get(start..=i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Reads a model's answer. Anything that does not describe working filters is
/// rejected as a whole; nothing is applied here.
pub fn parse_answer(text: &str, filters: &[Filter]) -> Option<Answer> {
    let raw: RawAnswer = serde_json::from_str(json_object(text)?).ok()?;
    let mut changes = Vec::with_capacity(raw.changes.len());
    for c in raw.changes {
        let index = || -> Option<usize> {
            let i = usize::try_from(c.band).ok()?.checked_sub(1)?;
            (i < filters.len()).then_some(i)
        };
        let change = match c.action {
            Action::Clear => Change::Clear,
            Action::Remove => Change::Remove(index()?),
            Action::Add | Action::Update => {
                let old = match c.action {
                    Action::Update => Some(index()?),
                    _ => None,
                };
                let base = old.map(|i| filters[i]);
                let kind = match &c.kind {
                    Some(name) => TYPES.iter().find(|(n, _)| n == name)?.1,
                    None => base?.kind,
                };
                let f = Filter {
                    kind,
                    freq_hz: c.freq_hz.or(base.map(|b| b.freq_hz))?,
                    gain_db: if kind.has_gain() { c.gain_db.or(base.map(|b| b.gain_db))? } else { 0.0 },
                    q: c.q.or(base.map(|b| b.q)).unwrap_or(1.0),
                    on: true,
                    channel: c.ear.or(base.map(|b| b.channel)).unwrap_or_default(),
                };
                if !f.is_valid() {
                    return None;
                }
                match old {
                    Some(i) if base == Some(f) => {
                        // Repeating a band unchanged is not a change.
                        let _ = i;
                        continue;
                    }
                    Some(i) => Change::Update(i, f),
                    None => Change::Add(f),
                }
            }
        };
        changes.push(change);
    }
    let reason: String = raw.reason.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(Answer { reason: reason.chars().take(400).collect(), changes })
}
