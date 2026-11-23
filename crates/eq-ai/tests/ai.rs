use eq_ai::*;
use eq_core::change::{Change, apply_changes};
use eq_core::filter::{Filter, Kind};
use eq_core::profile::Profile;
use proptest::prelude::*;
use std::cell::RefCell;

const KEY: &str = "sk-test-SECRET-123456";

struct Fake {
    replies: RefCell<Vec<Response>>,
    sent: RefCell<Vec<Request>>,
}

impl Fake {
    fn new(replies: &[(u16, &str)]) -> Self {
        let replies = replies.iter().rev().map(|(s, b)| Response { status: *s, body: (*b).into() }).collect();
        Self { replies: RefCell::new(replies), sent: RefCell::default() }
    }
}

impl Transport for Fake {
    fn send(&self, r: &Request) -> Result<Response, eq_ai::NetError> {
        self.sent.borrow_mut().push(r.clone());
        Ok(self.replies.borrow_mut().pop().expect("unexpected extra request"))
    }
}

fn config(provider: Provider) -> Config {
    Config { provider, url: String::new(), model: "some-model".into(), key: Some(KEY.into()) }
}

fn ctx() -> Context {
    Context { device: "Headphones".into(), filters: vec![Filter::peak(100.0, 2.0, 1.0)], ..Context::default() }
}

const GOOD: &str = r#"{"reason":"Brings voices closer.","changes":[{"action":"add","band":0,"type":"peaking","freq_hz":2200,"gain_db":3,"q":0.9,"ear":"both"},{"action":"remove","band":1,"type":"peaking","freq_hz":1000,"gain_db":0,"q":1,"ear":"both"}]}"#;

fn wrap(provider: Provider, text: &str) -> String {
    match provider {
        Provider::Anthropic => serde_json::json!({
            "stop_reason": "end_turn",
            "content": [{ "type": "thinking", "thinking": "" }, { "type": "text", "text": text }]
        }),
        _ => serde_json::json!({ "choices": [{ "message": { "role": "assistant", "content": text } }] }),
    }
    .to_string()
}

const PROVIDERS: [Provider; 3] = [Provider::Anthropic, Provider::OpenAi, Provider::Ollama];

#[test]
fn every_adapter_round_trips() {
    for p in PROVIDERS {
        let fake = Fake::new(&[(200, &wrap(p, GOOD))]);
        let a = ask(&fake, &config(p), &ctx(), "Vocals sound far away").unwrap();
        assert_eq!(a.changes, [Change::Add(Filter::peak(2200.0, 3.0, 0.9)), Change::Remove(0)]);
        let sent = fake.sent.borrow();
        let body = sent[0].body.as_ref().unwrap();
        assert!(body.contains("Vocals sound far away") && body.contains("freq_hz=100"));
        assert!(!body.contains(KEY), "the key belongs in a header only");
        assert!(!body.contains("temperature"));
        assert!(body.contains("json_schema"));
    }
}

#[test]
fn steps_down_when_structured_output_is_not_supported() {
    for p in [Provider::OpenAi, Provider::Ollama] {
        let fake = Fake::new(&[(400, "{}"), (422, "{}"), (200, &wrap(p, GOOD))]);
        assert!(ask(&fake, &config(p), &ctx(), "x").is_ok());
        assert!(!fake.sent.borrow()[2].body.as_ref().unwrap().contains("response_format"));
    }
    let p = Provider::Anthropic;
    let fake = Fake::new(&[(400, "{}"), (200, &wrap(p, &format!("Sure:\n```json\n{GOOD}\n```")))]);
    assert!(ask(&fake, &config(p), &ctx(), "x").is_ok());
    assert!(!fake.sent.borrow()[1].body.as_ref().unwrap().contains("output_config"));
}

#[test]
fn failures_become_plain_messages_without_the_key() {
    for p in PROVIDERS {
        for status in [301, 400, 401, 403, 404, 408, 422, 429, 500, 503, 529] {
            let body = format!(r#"{{"error":{{"message":"Incorrect API key provided: {KEY}. Bad {KEY}"}}}}"#);
            let replies = vec![(status, body.as_str()); 3];
            let e = ask(&Fake::new(&replies), &config(p), &ctx(), "x").unwrap_err();
            for text in [e.to_string(), format!("{e:?}"), format!("{:?}", config(p))] {
                assert!(!text.contains("SECRET"), "{text}");
            }
        }
    }
    let refused = r#"{"stop_reason":"refusal","content":[]}"#;
    assert_eq!(ask(&Fake::new(&[(200, refused)]), &config(Provider::Anthropic), &ctx(), "x"), Err(AiError::Declined));
    let refused = r#"{"choices":[{"message":{"refusal":"no","content":null}}]}"#;
    assert_eq!(ask(&Fake::new(&[(200, refused)]), &config(Provider::OpenAi), &ctx(), "x"), Err(AiError::Declined));
}

#[test]
fn keys_only_travel_over_safe_addresses() {
    let check = |url: &str, key: bool| {
        let mut c = config(Provider::OpenAi);
        c.url = url.into();
        if !key {
            c.key = None;
        }
        c.check_address().is_ok()
    };
    assert!(check("https://example.com/v1", true));
    assert!(check("http://localhost:1234/v1", true));
    assert!(check("http://127.0.0.1:1234/v1", true));
    assert!(check("http://[::1]:1234/v1", true));
    assert!(check("http://192.168.1.5:11434/v1", false));
    assert!(!check("http://192.168.1.5:11434/v1", true));
    assert!(!check("http://localhost.evil.com/v1", true));
    assert!(!check("http://localhost@evil.com/v1", true));
    assert!(!check("http://127.0.0.1.evil.com/v1", true));
    assert!(!check("ftp://example.com", true));
    assert!(!check("example.com", false));
    assert!(!check("https://", true));
}

#[test]
fn needs_key_and_model() {
    let mut c = config(Provider::Anthropic);
    c.key = None;
    assert!(matches!(c.check(), Err(AiError::NotConfigured(_))));
    let mut c = config(Provider::Ollama);
    c.key = None;
    assert!(c.check().is_ok());
    c.model = " ".into();
    assert!(c.check().is_err());
}

#[test]
fn lists_models() {
    let body = r#"{"data":[{"id":"b"},{"id":"a"},{"x":1}]}"#;
    assert_eq!(models(&Fake::new(&[(200, body)]), &config(Provider::OpenAi)).unwrap(), ["a", "b"]);
    assert_eq!(models(&Fake::new(&[(200, body)]), &config(Provider::Anthropic)).unwrap(), ["b", "a"]);
    assert!(models(&Fake::new(&[(200, "nope")]), &config(Provider::Ollama)).is_err());
}

#[test]
fn rejects_unusable_answers() {
    let f = [Filter::peak(100.0, 2.0, 1.0)];
    for bad in [
        "",
        "hello",
        "{",
        "{}",
        r#"{"changes":"none"}"#,
        r#"{"changes":[{"action":"explode"}]}"#,
        r#"{"changes":[{"action":"add","type":"peaking","freq_hz":-5,"gain_db":1,"q":1}]}"#,
        r#"{"changes":[{"action":"add","type":"peaking","freq_hz":100,"gain_db":1,"q":0}]}"#,
        r#"{"changes":[{"action":"add","type":"wobble","freq_hz":100,"gain_db":1,"q":1}]}"#,
        r#"{"changes":[{"action":"update","band":2,"gain_db":1}]}"#,
        r#"{"changes":[{"action":"remove","band":-1}]}"#,
        r#"{"changes":[{"action":"remove","band":0}]}"#,
        r#"{"changes":[{"action":"add","type":"peaking","freq_hz":1e999,"gain_db":1,"q":1}]}"#,
        r#"{"changes":[],"run":"rm"}"#,
    ] {
        assert_eq!(parse_answer(bad, &f), None, "{bad}");
    }
    let partial = parse_answer(r#"{"changes":[{"action":"update","band":1,"gain_db":5}]}"#, &f).unwrap();
    assert_eq!(partial.changes, [Change::Update(0, Filter { gain_db: 5.0, ..f[0] })]);
    let empty = parse_answer(r#"{"reason":"Not about sound.","changes":[]}"#, &f).unwrap();
    assert!(empty.changes.is_empty());
    let big = parse_answer(r#"{"changes":[{"action":"add","type":"low_shelf","freq_hz":60,"gain_db":40,"q":1}]}"#, &f).unwrap();
    assert_eq!(big.changes, [Change::Add(Filter::new(Kind::LowShelf, 60.0, 40.0, 1.0))], "warned about, not clamped");
}

#[test]
fn hearing_data_is_only_sent_when_allowed() {
    let p = Profile {
        hearing: Some(eq_core::HearingLayer {
            on: true,
            strength: 1.0,
            left: vec![Filter::peak(4000.0, 7.5, 1.2)],
            right: vec![],
        }),
        ..Profile::default()
    };
    assert!(!Context::new(&p, "d", false).summary().contains("4000"));
    assert!(Context::new(&p, "d", true).summary().contains("4000"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]
    #[test]
    fn broken_answers_never_produce_broken_filters(
        noise in "\\PC{0,40}",
        freq in prop::num::f64::ANY, gain in prop::num::f64::ANY, q in prop::num::f64::ANY, band in any::<i64>(),
        action in "(add|update|remove|clear|x)", cut in 0usize..200,
    ) {
        let text = format!(r#"{noise}{{"reason":"{}","changes":[{{"action":"{action}","band":{band},"type":"peaking","freq_hz":{freq:e},"gain_db":{gain:e},"q":{q:e},"ear":"both"}}]}}"#, "ok");
        let cut_text: String = text.chars().take(cut).collect();
        let filters = [Filter::peak(100.0, 2.0, 1.0)];
        for t in [&text, &cut_text, &noise] {
            if let Some(a) = parse_answer(t, &filters) {
                let mut p = Profile { filters: filters.to_vec(), ..Profile::default() };
                prop_assert!(apply_changes(&mut p, &a.changes).is_ok());
                prop_assert!(p.filters.iter().all(Filter::is_valid));
                prop_assert!(p.chain(48_000.0).preamp_db.is_finite());
            }
        }
    }
}
