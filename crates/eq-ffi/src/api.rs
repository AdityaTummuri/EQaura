//! What the app can ask for.

use crate::bands;
use eq_ai::{Config, Context, Http, Provider};
use eq_core::change::{self, Change};
use eq_core::curve::{self, Grid};
use eq_core::filter::{Channel, Filter};
use eq_core::hearing::{self, Ear, Test, Tone};
use eq_core::profile::{Correction, HearingLayer, Profile};
use eq_core::warning::{self, Severity};
use eq_core::{autoeq, builtin, preset, rules, sliders, text};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

const RATE: f64 = 48_000.0;

type Reply = Result<Value, String>;

/// `{"ok": …}` or `{"error": "…"}`.
pub fn call(method: &str, args: &str) -> String {
    let reply = match method {
        "analyze" => with(args, analyze),
        "engine" => with(args, engine),
        "suggest" => with(args, suggest),
        "ask" => with(args, ask),
        "models" => with(args, models),
        "sends" => with(args, sends),
        "search" => with(args, search),
        "headphone" => with(args, headphone),
        "hearing" => with(args, hearing_test),
        "ear_bars" => with(args, ear_bars),
        "import" => with(args, import),
        "export" => with(args, export),
        "read_profile" => with(args, read_profile),
        "slider" => with(args, slider),
        "number" => with(args, number),
        "builtin" => Ok(json!(builtin::builtin())),
        "version" => Ok(json!(env!("CARGO_PKG_VERSION"))),
        _ => Err(format!("Unknown call {method}")),
    };
    match reply {
        Ok(v) => json!({ "ok": v }),
        Err(e) => json!({ "error": e }),
    }
    .to_string()
}

fn with<T: for<'a> Deserialize<'a>>(args: &str, f: impl FnOnce(T) -> Reply) -> Reply {
    f(serde_json::from_str(args).map_err(|e| format!("Bad arguments: {e}"))?)
}

pub fn tone(left: bool, freq: f64, db: f64, rate: f64) -> Vec<f32> {
    let ear = if left { Ear::Left } else { Ear::Right };
    if !(8_000.0..=384_000.0).contains(&rate) || !freq.is_finite() || !db.is_finite() {
        return Vec::new();
    }
    // Nothing in the hearing test is louder than -10 dB.
    hearing::render_tone(Tone { ear, freq, db: db.min(-10.0) }, rate)
}

// ---- sound ----

#[derive(Deserialize)]
struct Analyze {
    profile: Profile,
    points: usize,
    selected: Option<usize>,
}

fn analyze(a: Analyze) -> Reply {
    let p = &a.profile;
    let grid = Grid::new(curve::log_grid(curve::MIN_HZ, curve::MAX_HZ, a.points.clamp(2, 2000)), RATE);
    let chain = p.chain(RATE);
    let (correction, _) = p.correction_chain();
    let selected = a
        .selected
        .and_then(|i| p.filters.get(i))
        .filter(|f| f.is_active())
        .map(|f| grid.response(&[Filter { channel: Channel::Both, ..*f }]));
    Ok(json!({
        "preamp_db": chain.preamp_db,
        "net_peak_db": chain.net_peak_db(),
        "left": grid.response(&chain.left),
        "right": (!chain.is_symmetric()).then(|| grid.response(&chain.right)),
        "correction": (!correction.is_empty()).then(|| grid.response(&correction)),
        "selected": selected,
        "sliders": (0..sliders::FREQS.len()).map(|i| sliders::position(&p.filters, i)).collect::<Vec<_>>(),
        "warnings": warning::check(p, RATE).iter().map(|w| json!({
            "severity": match w.severity {
                Severity::Info => "info",
                Severity::Caution => "caution",
                Severity::Danger => "danger",
            },
            "title": w.title,
            "detail": w.detail,
            "band": w.band,
        })).collect::<Vec<_>>(),
    }))
}

#[derive(Deserialize)]
struct Engine {
    profile: Profile,
    block: usize,
    rate: f64,
    /// Same preamp, no filters: a fair comparison at one volume.
    flat: bool,
}

fn engine(a: Engine) -> Reply {
    if !a.block.is_power_of_two() || !(128..=32_768).contains(&a.block) {
        return Err("Bad block size".into());
    }
    if !(8_000.0..=384_000.0).contains(&a.rate) {
        return Err("Bad sample rate".into());
    }
    let chain = a.profile.chain(a.rate);
    let side = |f: &[Filter]| bands::gains(if a.flat { &[] } else { f }, a.block, a.rate);
    Ok(json!({
        "preamp_db": chain.preamp_db,
        "cutoffs": bands::cutoffs(a.block, a.rate),
        "left": side(&chain.left),
        "right": side(&chain.right),
    }))
}

#[derive(Deserialize)]
struct SliderArgs {
    filters: Vec<Filter>,
    index: usize,
    gain_db: f64,
}

fn slider(a: SliderArgs) -> Reply {
    let mut filters = a.filters;
    sliders::set(&mut filters, a.index, a.gain_db);
    Ok(json!(filters))
}

#[derive(Deserialize)]
struct Text {
    text: String,
}

fn number(a: Text) -> Reply {
    Ok(json!(text::parse_number(&a.text)))
}

// ---- presets ----

#[derive(Deserialize)]
struct Import {
    name: String,
    text: String,
}

fn import(a: Import) -> Reply {
    if a.text.trim_start().starts_with('{') {
        return Ok(json!({ "profile": Profile::from_json(&a.text)?, "skipped": 0 }));
    }
    let parsed = preset::parse(&a.text);
    let skipped = parsed.skipped.len();
    if parsed.filters.is_empty() && parsed.preamp_db.is_none() {
        return Err("No equalizer settings were found in this file".into());
    }
    Ok(json!({ "profile": parsed.into_profile(&a.name), "skipped": skipped }))
}

#[derive(Deserialize)]
struct ProfileArgs {
    profile: Profile,
}

fn export(a: ProfileArgs) -> Reply {
    Ok(json!(preset::export(&a.profile)))
}

/// Reads a stored profile of any known version.
fn read_profile(a: Text) -> Reply {
    Ok(json!(Profile::from_json(&a.text)?))
}

// ---- headphones ----

#[derive(Deserialize)]
struct Search {
    query: String,
    limit: usize,
}

fn search(a: Search) -> Reply {
    let hits = autoeq::search(&a.query);
    let rows: Vec<Value> = hits
        .iter()
        .take(a.limit)
        .filter_map(|i| {
            let e = autoeq::get(*i)?;
            Some(json!({ "index": i, "name": e.name, "source": e.source, "form": e.form }))
        })
        .collect();
    Ok(json!({ "total": hits.len(), "rows": rows }))
}

#[derive(Deserialize)]
struct Index {
    index: usize,
}

fn headphone(a: Index) -> Reply {
    let e = autoeq::get(a.index).ok_or("This headphone is not in the list")?;
    Ok(json!(Correction {
        name: e.name.into(),
        source: e.source.into(),
        on: true,
        strength: 1.0,
        filters: e.filters(),
    }))
}

// ---- hearing test ----

#[derive(Deserialize)]
struct Answers {
    answers: Vec<bool>,
}

/// Replays all answers so far and says what comes next.
fn hearing_test(a: Answers) -> Reply {
    let mut test = Test::new();
    a.answers.iter().for_each(|heard| test.answer(*heard));
    if let Some(t) = test.tone() {
        return Ok(json!({
            "done": false,
            "progress": test.progress(),
            "left": t.ear == Ear::Left,
            "freq": t.freq,
            "db": t.db,
            "label": text::freq(t.freq),
        }));
    }
    let thresholds = test.thresholds();
    let comp = hearing::compensation(&thresholds);
    let left: Vec<(f64, f64)> = comp.iter().map(|c| (c.0, c.1)).collect();
    let right: Vec<(f64, f64)> = comp.iter().map(|c| (c.0, c.2)).collect();
    let bars: Vec<Value> = comp
        .iter()
        .zip(&thresholds)
        .map(|(c, t)| {
            json!({
                "label": text::freq(c.0),
                "left": c.1,
                "right": c.2,
                "tested": t.left.is_some() && t.right.is_some(),
            })
        })
        .collect();
    Ok(json!({
        "done": true,
        "progress": 1.0,
        "bars": bars,
        "layer": HearingLayer {
            on: true,
            strength: 1.0,
            left: hearing::fit(&left, RATE),
            right: hearing::fit(&right, RATE),
        },
    }))
}

#[derive(Deserialize)]
struct Layer {
    layer: HearingLayer,
}

fn ear_bars(a: Layer) -> Reply {
    let mut freqs = hearing::FREQS;
    freqs.sort_by(f64::total_cmp);
    Ok(json!(
        freqs
            .iter()
            .map(|f| json!({
                "label": text::freq(*f),
                "left": curve::response_at(&a.layer.left, *f, RATE),
                "right": curve::response_at(&a.layer.right, *f, RATE),
                "tested": true,
            }))
            .collect::<Vec<_>>()
    ))
}

// ---- assistant ----

#[derive(Deserialize)]
struct Suggest {
    words: String,
    profile: Profile,
}

/// `null` when the words were not understood.
fn suggest(a: Suggest) -> Reply {
    match rules::suggest(&a.words, &a.profile.filters) {
        Some(s) => proposal(&a.profile, &s.reason, &s.changes),
        None => Ok(Value::Null),
    }
}

/// What a suggestion would do, ready to show and to apply.
fn proposal(profile: &Profile, reason: &str, changes: &[Change]) -> Reply {
    let mut preview = profile.clone();
    change::apply_changes(&mut preview, changes).map_err(|_| "The suggestion could not be used.")?;
    let old = warning::check(profile, RATE);
    let notes: Vec<String> = warning::check(&preview, RATE)
        .iter()
        .filter(|w| w.severity > Severity::Info && !old.iter().any(|o| o.title == w.title))
        .map(|w| format!("{}. {}", w.title, w.detail))
        .collect();
    Ok(json!({
        "reason": if reason.is_empty() { "Here is a suggestion." } else { reason },
        "lines": changes.iter().map(|c| change::describe_change(&profile.filters, c)).collect::<Vec<_>>(),
        "notes": notes,
        "after": preview.filters,
    }))
}

#[derive(Deserialize)]
struct Ai {
    provider: String,
    url: String,
    model: String,
    key: Option<String>,
}

impl Ai {
    fn config(self) -> Result<Config, String> {
        let provider = match self.provider.as_str() {
            "anthropic" => Provider::Anthropic,
            "openai" => Provider::OpenAi,
            "ollama" => Provider::Ollama,
            _ => return Err("Choose a provider in Settings first.".into()),
        };
        let key = self.key.filter(|k| !k.is_empty());
        Ok(Config { provider, url: self.url, model: self.model, key })
    }
}

#[derive(Deserialize)]
struct Ask {
    ai: Ai,
    words: String,
    device: String,
    send_hearing: bool,
    /// Earlier requests in this chat with the reason given for each answer.
    earlier: Vec<(String, String)>,
    profile: Profile,
}

fn ask(a: Ask) -> Reply {
    let config = a.ai.config()?;
    let mut ctx = Context::new(&a.profile, &a.device, a.send_hearing);
    ctx.earlier = a.earlier;
    let http = Http::new(Duration::from_secs(60));
    let answer = eq_ai::ask(&http, &config, &ctx, &a.words).map_err(|e| e.to_string())?;
    proposal(&a.profile, &answer.reason, &answer.changes)
}

fn models(a: Ai) -> Reply {
    let config = a.config()?;
    let list = eq_ai::models(&Http::new(Duration::from_secs(20)), &config).map_err(|e| e.to_string())?;
    Ok(json!(list))
}

#[derive(Deserialize)]
struct Sends {
    profile: Profile,
    device: String,
    send_hearing: bool,
}

/// The exact text that goes along with a request.
fn sends(a: Sends) -> Reply {
    Ok(json!(Context::new(&a.profile, &a.device, a.send_hearing).summary()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(method: &str, args: Value) -> Value {
        let v: Value = serde_json::from_str(&call(method, &args.to_string())).unwrap();
        assert!(v.get("error").is_none(), "{method}: {v}");
        v["ok"].clone()
    }

    fn profile() -> Value {
        json!({ "schema": 1, "name": "Test", "filters": [
            { "type": "low_shelf", "freq_hz": 105, "gain_db": 4.0, "q": 0.7 }
        ]})
    }

    #[test]
    fn analyze_gives_curve_and_preamp() {
        let v = ok("analyze", json!({ "profile": profile(), "points": 64, "selected": 0 }));
        assert_eq!(v["left"].as_array().unwrap().len(), 64);
        assert!(v["right"].is_null());
        assert!(v["net_peak_db"].as_f64().unwrap() <= 1e-6);
        assert!(v["preamp_db"].as_f64().unwrap() < -3.9);
        assert_eq!(v["selected"].as_array().unwrap().len(), 64);
    }

    #[test]
    fn engine_matches_lengths_and_flat_keeps_the_preamp() {
        let args = |flat| json!({ "profile": profile(), "block": 2048, "rate": 48000, "flat": flat });
        let on = ok("engine", args(false));
        let flat = ok("engine", args(true));
        let n = on["cutoffs"].as_array().unwrap().len();
        assert_eq!(on["left"].as_array().unwrap().len(), n);
        assert_eq!(on["preamp_db"], flat["preamp_db"]);
        assert!(flat["left"].as_array().unwrap().iter().all(|g| g == 0.0));
        assert!(call("engine", &json!({ "profile": profile(), "block": 1000, "rate": 48000, "flat": false }).to_string()).contains("error"));
    }

    #[test]
    fn rules_make_a_usable_proposal() {
        let v = ok("suggest", json!({ "words": "more bass, less harsh", "profile": profile() }));
        assert!(!v["lines"].as_array().unwrap().is_empty());
        assert_ne!(v["after"], profile()["filters"]);
        assert!(ok("suggest", json!({ "words": "qwertz", "profile": profile() })).is_null());
    }

    #[test]
    fn headphones_can_be_found_and_used() {
        let v = ok("search", json!({ "query": "hd600", "limit": 5 }));
        let rows = v["rows"].as_array().unwrap();
        assert!(!rows.is_empty() && rows.len() <= 5);
        let h = ok("headphone", json!({ "index": rows[0]["index"] }));
        assert!(!h["filters"].as_array().unwrap().is_empty());
    }

    #[test]
    fn hearing_test_reaches_an_end() {
        let mut answers: Vec<bool> = Vec::new();
        for i in 0..2000 {
            let v = ok("hearing", json!({ "answers": answers }));
            if v["done"] == true {
                assert_eq!(v["bars"].as_array().unwrap().len(), 8);
                let bars = ok("ear_bars", json!({ "layer": v["layer"] }));
                assert_eq!(bars.as_array().unwrap().len(), 8);
                return;
            }
            answers.push(i % 3 != 0);
        }
        panic!("the test never ended");
    }

    #[test]
    fn presets_round_trip() {
        let text = ok("export", json!({ "profile": profile() }));
        let back = ok("import", json!({ "name": "Back", "text": text }));
        assert_eq!(back["profile"]["filters"].as_array().unwrap().len(), 1);
        assert_eq!(back["profile"]["preamp"]["auto"], true);
        assert!(ok("builtin", json!({})).as_array().unwrap().len() >= 10);
        assert!(call("import", r#"{"name":"x","text":"hello"}"#).contains("error"));
    }

    #[test]
    fn nonsense_never_panics() {
        for m in ["analyze", "engine", "ask", "models", "nope", "hearing", "import"] {
            for a in ["", "{}", "[]", "null", r#"{"profile":{"schema":99}}"#] {
                assert!(call(m, a).contains("error"), "{m} {a}");
            }
        }
        assert!(tone(true, 1000.0, 20.0, 48_000.0).iter().all(|s| s.abs() <= 0.32));
        assert!(tone(true, f64::NAN, 0.0, 1.0).is_empty());
    }
}
