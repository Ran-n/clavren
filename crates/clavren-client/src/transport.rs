// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:11.787103
//+ Revised: 	2026/11/03 04:54:11.787103
// -----------------------------------------------------------------------------

//! Talking to the server: `POST` JSON, get a status and JSON back. Behind a trait so tests can
//! answer without a network.

use std::time::Duration;

use serde_json::Value;

pub struct Answer {
    pub status: u16,
    pub body: Value,
    /// `Retry-After`, in seconds, when the server sent one.
    pub retry_after: Option<u64>,
}

pub trait Transport: Send + Sync {
    /// `Err` only when no answer arrived (no network, timeout, refused connection).
    fn post(&self, path: &str, body: &Value) -> Result<Answer, String>;
}

impl<T: Transport + ?Sized> Transport for std::sync::Arc<T> {
    fn post(&self, path: &str, body: &Value) -> Result<Answer, String> {
        (**self).post(path, body)
    }
}

/// HTTPS with a 15-second limit per request.
pub struct HttpTransport {
    agent: ureq::Agent,
    base_url: String,
}

pub const TIMEOUT: Duration = Duration::from_secs(15);

impl HttpTransport {
    /// `base_url` is the service's address, e.g. `https://licences.example.com`.
    pub fn new(base_url: &str) -> Self {
        let config = ureq::Agent::config_builder().timeout_global(Some(TIMEOUT)).http_status_as_error(false).build();
        Self { agent: config.into(), base_url: base_url.trim_end_matches('/').to_string() }
    }
}

impl Transport for HttpTransport {
    fn post(&self, path: &str, body: &Value) -> Result<Answer, String> {
        let url = format!("{}{path}", self.base_url);
        let mut response = self
            .agent
            .post(&url)
            .header("content-type", "application/json")
            .send(body.to_string())
            .map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let retry_after = response.headers().get("retry-after").and_then(|value| value.to_str().ok()?.parse().ok());
        let text = response.body_mut().read_to_string().map_err(|error| error.to_string())?;
        // a proxy's HTML error page isn't JSON; the status still says what happened
        Ok(Answer { status, body: serde_json::from_str(&text).unwrap_or(Value::Null), retry_after })
    }
}
