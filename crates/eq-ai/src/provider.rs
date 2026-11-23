use crate::answer;
use crate::http::{Request, Response};
use crate::{AiError, Context};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    OpenAi,
    Ollama,
}

impl Provider {
    pub fn default_url(self) -> &'static str {
        match self {
            Provider::Anthropic => "https://api.anthropic.com",
            Provider::OpenAi => "https://api.openai.com/v1",
            Provider::Ollama => "http://localhost:11434/v1",
        }
    }

    pub fn needs_key(self) -> bool {
        !matches!(self, Provider::Ollama)
    }

    pub(crate) fn levels(self) -> u8 {
        match self {
            Provider::Anthropic => 2,
            Provider::OpenAi | Provider::Ollama => 3,
        }
    }
}

#[derive(Clone)]
pub struct Config {
    pub provider: Provider,
    pub url: String,
    pub model: String,
    pub key: Option<String>,
}

// The key must never reach a log line by accident.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("provider", &self.provider)
            .field("url", &self.url)
            .field("model", &self.model)
            .field("key", &self.key.as_ref().map(|_| "hidden"))
            .finish()
    }
}

const SYSTEM: &str = "You tune a parametric equalizer. The user describes in plain words how their sound should change. \
Answer with one JSON object: {\"reason\": string, \"changes\": [...]}. \
reason is one short sentence in the user's language, with no jargon. \
Each change has: action (add, update, remove or clear), band (the band number for update and remove, otherwise 0), \
type (peaking, low_shelf, high_shelf, low_pass, high_pass or notch), freq_hz, gain_db, q, ear (both, left or right). \
update replaces the whole band, so repeat the values that stay the same. clear removes all bands. \
For remove and clear, set type to peaking, freq_hz to 1000, gain_db to 0, q to 1 and ear to both. \
Prefer adjusting existing bands over adding overlapping ones, use as few changes as needed, \
and make moderate moves (about 2 to 4 dB) unless the user asks for more. Cuts often work better than boosts. \
The app sets the preamp itself, so do not compensate for loudness. \
If the request is not about sound, return an empty changes list and say so in reason.";

impl Config {
    fn base(&self) -> String {
        let u = self.url.trim();
        let u = if u.is_empty() { self.provider.default_url() } else { u };
        u.trim_end_matches('/').to_owned()
    }

    pub fn check_address(&self) -> Result<(), AiError> {
        let base = self.base();
        let lower = base.to_ascii_lowercase();
        let host = lower.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("");
        // "user:pass@host" style addresses could hide the real host.
        if host.is_empty() || host.contains('@') || lower.contains(char::is_whitespace) {
            return Err(AiError::NotConfigured("The provider address is not valid.".into()));
        }
        let bare = if host.starts_with('[') {
            host.split(']').next().unwrap_or("").trim_start_matches('[')
        } else {
            host.split(':').next().unwrap_or("")
        };
        let local = bare == "localhost"
            || bare == "::1"
            || bare.parse::<std::net::Ipv4Addr>().is_ok_and(|ip| ip.is_loopback());
        if lower.starts_with("https://") || (lower.starts_with("http://") && (local || self.key.is_none())) {
            Ok(())
        } else if lower.starts_with("http://") {
            Err(AiError::NotConfigured("Use an https address so your API key stays private.".into()))
        } else {
            Err(AiError::NotConfigured("The provider address must start with https://".into()))
        }
    }

    pub fn check(&self) -> Result<(), AiError> {
        self.check_address()?;
        let default_host = self.base() == self.provider.default_url();
        if self.provider.needs_key() && self.key.is_none() && default_host {
            return Err(AiError::NotConfigured("Add an API key in Settings first.".into()));
        }
        if self.model.trim().is_empty() {
            return Err(AiError::NotConfigured("Choose a model in Settings first.".into()));
        }
        Ok(())
    }

    fn headers(&self) -> Vec<(&'static str, String)> {
        let mut h = Vec::new();
        if self.provider == Provider::Anthropic {
            h.push(("anthropic-version", "2026-10-21".into()));
        }
        if let Some(k) = &self.key {
            match self.provider {
                Provider::Anthropic => h.push(("x-api-key", k.clone())),
                _ => h.push(("authorization", format!("Bearer {k}"))),
            }
        }
        h
    }

    fn messages(&self, ctx: &Context, words: &str) -> Vec<Value> {
        let mut out = Vec::new();
        for (request, reason) in ctx.earlier.iter().rev().take(6).rev() {
            out.push(json!({ "role": "user", "content": request }));
            let shown = json!({ "reason": reason, "changes": [] }).to_string();
            out.push(json!({ "role": "assistant", "content": shown }));
        }
        let now = format!("Current settings:\n{}\nRequest: {}", ctx.summary(), words.trim());
        out.push(json!({ "role": "user", "content": now }));
        out
    }

    /// `level` 0 asks for schema-checked output, higher levels ask for less.
    pub fn ask_request(&self, ctx: &Context, words: &str, level: u8) -> Request {
        let mut messages = self.messages(ctx, words);
        let body = match self.provider {
            Provider::Anthropic => {
                let mut b = json!({
                    "model": self.model.trim(),
                    "max_tokens": 16000,
                    "system": SYSTEM,
                    "messages": messages,
                });
                if level == 0 {
                    b["output_config"] = json!({
                        "effort": "low",
                        "format": { "type": "json_schema", "schema": answer::schema() },
                    });
                }
                b
            }
            Provider::OpenAi | Provider::Ollama => {
                messages.insert(0, json!({ "role": "system", "content": SYSTEM }));
                let mut b = json!({ "model": self.model.trim(), "messages": messages, "stream": false });
                match level {
                    0 => {
                        b["response_format"] = json!({
                            "type": "json_schema",
                            "json_schema": { "name": "eq_changes", "strict": true, "schema": answer::schema() },
                        });
                    }
                    1 => b["response_format"] = json!({ "type": "json_object" }),
                    _ => {}
                }
                b
            }
        };
        let path = match self.provider {
            Provider::Anthropic => "/v1/messages",
            _ => "/chat/completions",
        };
        Request { url: self.base() + path, headers: self.headers(), body: Some(body.to_string()) }
    }

    pub fn models_request(&self) -> Request {
        let path = match self.provider {
            Provider::Anthropic => "/v1/models?limit=100",
            _ => "/models",
        };
        Request { url: self.base() + path, headers: self.headers(), body: None }
    }

    /// Removes the key, and anything shaped like one, from provider messages.
    fn redact(&self, text: &str) -> String {
        let mut s = text.to_owned();
        if let Some(k) = self.key.as_deref().filter(|k| !k.is_empty()) {
            s = s.replace(k, "[key hidden]");
        }
        s.split_whitespace()
            .map(|w| if w.contains("sk-") { "[key hidden]" } else { w })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn status_error(&self, r: &Response) -> Option<AiError> {
        if (200..300).contains(&r.status) {
            return None;
        }
        let v: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
        let message = v
            .pointer("/error/message")
            .or_else(|| v.get("error"))
            .or_else(|| v.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let message: String = self.redact(message).chars().take(200).collect();
        Some(match r.status {
            401 | 403 => AiError::BadKey,
            404 => AiError::NotFound(message),
            402 | 429 => AiError::RateLimited,
            408 | 504 => AiError::Timeout,
            code => AiError::Server(code, message),
        })
    }

    pub fn read_answer(&self, r: &Response) -> Result<String, AiError> {
        if let Some(e) = self.status_error(r) {
            return Err(e);
        }
        let v: Value = serde_json::from_str(&r.body).map_err(|_| AiError::BadAnswer)?;
        match self.provider {
            Provider::Anthropic => {
                if v.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
                    return Err(AiError::Declined);
                }
                let text: String = v
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect();
                if text.is_empty() { Err(AiError::BadAnswer) } else { Ok(text) }
            }
            Provider::OpenAi | Provider::Ollama => {
                let m = v.pointer("/choices/0/message").ok_or(AiError::BadAnswer)?;
                if m.get("refusal").and_then(Value::as_str).is_some_and(|s| !s.is_empty()) {
                    return Err(AiError::Declined);
                }
                m.get("content").and_then(Value::as_str).map(str::to_owned).ok_or(AiError::BadAnswer)
            }
        }
    }

    pub fn read_models(&self, r: &Response) -> Result<Vec<String>, AiError> {
        if let Some(e) = self.status_error(r) {
            return Err(e);
        }
        let v: Value = serde_json::from_str(&r.body).map_err(|_| AiError::BadAnswer)?;
        let mut names: Vec<String> = v
            .get("data")
            .and_then(Value::as_array)
            .ok_or(AiError::BadAnswer)?
            .iter()
            .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect();
        // Anthropic lists newest first; other lists have no useful order.
        if self.provider != Provider::Anthropic {
            names.sort();
        }
        Ok(names)
    }
}
