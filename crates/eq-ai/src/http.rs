use crate::AiError;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    /// `None` makes a GET request.
    pub body: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetError {
    Timeout,
    Offline,
}

pub(crate) fn net_error(e: NetError) -> AiError {
    match e {
        NetError::Timeout => AiError::Timeout,
        NetError::Offline => AiError::Offline,
    }
}

/// Lets tests run every adapter without a network.
pub trait Transport {
    fn send(&self, request: &Request) -> Result<Response, NetError>;
}

pub struct Http {
    agent: ureq::Agent,
}

impl Http {
    pub fn new(timeout: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            // A redirect could carry the key to another host.
            .max_redirects(0)
            .max_redirects_will_error(false)
            .user_agent("EQaura")
            .build()
            .into();
        Self { agent }
    }
}

impl Transport for Http {
    fn send(&self, request: &Request) -> Result<Response, NetError> {
        let map = |e: ureq::Error| match e {
            ureq::Error::Timeout(_) => NetError::Timeout,
            _ => NetError::Offline,
        };
        let result = match &request.body {
            Some(body) => {
                let mut r = self.agent.post(&request.url).header("content-type", "application/json");
                for (k, v) in &request.headers {
                    r = r.header(*k, v);
                }
                r.send(body.as_str())
            }
            None => {
                let mut r = self.agent.get(&request.url);
                for (k, v) in &request.headers {
                    r = r.header(*k, v);
                }
                r.call()
            }
        };
        let mut response = result.map_err(map)?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(4 * 1024 * 1024)
            .lossy_utf8(true)
            .read_to_string()
            .map_err(map)?;
        Ok(Response { status, body })
    }
}
