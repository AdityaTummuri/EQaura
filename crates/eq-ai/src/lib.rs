//! Optional AI tuning with the user's own key. Requests go straight from this
//! computer to the provider the user picked; there is no server in between.

mod answer;
mod http;
mod provider;

pub use answer::{Answer, parse_answer};
pub use http::{Http, NetError, Request, Response, Transport};
pub use provider::{Config, Provider};

use eq_core::filter::{Channel, Filter};
use eq_core::profile::Profile;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiError {
    NotConfigured(String),
    BadKey,
    NotFound(String),
    RateLimited,
    Timeout,
    Offline,
    Server(u16, String),
    Declined,
    BadAnswer,
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::NotConfigured(s) => write!(f, "{s}"),
            AiError::BadKey => write!(f, "The provider did not accept this API key."),
            AiError::NotFound(s) => write!(f, "The provider could not find this model or address. {s}"),
            AiError::RateLimited => {
                write!(
                    f,
                    "The provider is limiting requests, or the account is out of credit. Try again later."
                )
            }
            AiError::Timeout => write!(f, "The provider took too long to answer."),
            AiError::Offline => write!(f, "Could not reach the provider. Check the connection and address."),
            AiError::Server(code, s) => write!(f, "The provider returned an error ({code}). {s}"),
            AiError::Declined => write!(f, "The model declined this request."),
            AiError::BadAnswer => write!(f, "The answer could not be used as equalizer changes."),
        }
    }
}

/// Everything a request may contain. Nothing else is sent.
#[derive(Clone, Debug, Default)]
pub struct Context {
    pub device: String,
    pub headphone: Option<String>,
    pub filters: Vec<Filter>,
    /// Only set when the user allows it.
    pub hearing: Option<String>,
    /// Earlier requests in this chat with the reason given for each answer.
    pub earlier: Vec<(String, String)>,
}

impl Context {
    pub fn new(profile: &Profile, device: &str, send_hearing: bool) -> Self {
        let hearing = profile.hearing.as_ref().filter(|_| send_hearing).map(|h| {
            let side = |v: &[Filter]| {
                v.iter()
                    .map(|f| format!("{} Hz {:+.1} dB", f.freq_hz.round(), f.gain_db * h.strength))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!("left ear: {}; right ear: {}", side(&h.left), side(&h.right))
        });
        Self {
            device: device.into(),
            headphone: profile.headphone.as_ref().filter(|h| h.on).map(|h| h.name.clone()),
            filters: profile.filters.clone(),
            hearing,
            earlier: Vec::new(),
        }
    }

    /// The exact text sent along with the user's words.
    pub fn summary(&self) -> String {
        let mut s = format!("Output device: {}\n", self.device);
        if let Some(h) = &self.headphone {
            s += &format!("Headphone correction already active (not listed below): {h}\n");
        }
        if let Some(h) = &self.hearing {
            s += &format!("Hearing compensation already active (not listed below): {h}\n");
        }
        if self.filters.is_empty() {
            s += "Bands: none\n";
        } else {
            s += "Bands:\n";
            for (i, f) in self.filters.iter().enumerate() {
                let ear = match f.channel {
                    Channel::Both => "both",
                    Channel::Left => "left",
                    Channel::Right => "right",
                };
                s += &format!(
                    "{}. {} freq_hz={} gain_db={} q={} ear={}{}\n",
                    i + 1,
                    answer::type_name(f.kind),
                    round(f.freq_hz),
                    round(f.gain_db),
                    round(f.q),
                    ear,
                    if f.on { "" } else { " (off)" }
                );
            }
        }
        s
    }
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Asks the model for changes. Blocking: call from a background thread.
pub fn ask(http: &dyn Transport, config: &Config, ctx: &Context, words: &str) -> Result<Answer, AiError> {
    config.check()?;
    let mut last = AiError::BadAnswer;
    // Some servers reject structured output. Step down until one form works.
    for level in 0..config.provider.levels() {
        let response = http.send(&config.ask_request(ctx, words, level)).map_err(http::net_error)?;
        match config.read_answer(&response) {
            Ok(text) => return parse_answer(&text, &ctx.filters).ok_or(AiError::BadAnswer),
            Err(e @ AiError::Server(400 | 422, _)) => last = e,
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

/// Model names the provider offers.
pub fn models(http: &dyn Transport, config: &Config) -> Result<Vec<String>, AiError> {
    config.check_address()?;
    let response = http.send(&config.models_request()).map_err(http::net_error)?;
    config.read_models(&response)
}
