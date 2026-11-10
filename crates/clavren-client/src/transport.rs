// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:11.787103
//+ Revised: 	2026/11/09 22:30:53.310749
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

/// HTTPS with a 15-second limit per request. Plain `http://` only to this computer (tests, a
/// server run locally): the licence key travels in the activation request.
pub struct HttpTransport {
    agent: ureq::Agent,
    base_url: String,
    /// Why requests to `base_url` are refused, when it is neither HTTPS nor this computer.
    refused: Option<String>,
}

pub const TIMEOUT: Duration = Duration::from_secs(15);

impl HttpTransport {
    /// `base_url` is the service's address, e.g. `https://licences.example.com`.
    pub fn new(base_url: &str) -> Self {
        let config = ureq::Agent::config_builder().timeout_global(Some(TIMEOUT)).http_status_as_error(false).build();
        let refused = (!secure_or_local(base_url)).then(|| {
            let reason = format!("the licence server's address must start with https:// (it is {base_url})");
            log::error!("{reason}");
            reason
        });
        Self { agent: config.into(), base_url: base_url.trim_end_matches('/').to_string(), refused }
    }
}

/// HTTPS, or plain HTTP to this computer.
fn secure_or_local(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return true;
    }
    let Some(rest) = lower.strip_prefix("http://") else { return false };
    let host_and_port = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = match host_and_port.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
        None => host_and_port.split(':').next().unwrap_or_default(),
    };
    host == "localhost" || host.parse::<std::net::IpAddr>().is_ok_and(|address| address.is_loopback())
}

impl Transport for HttpTransport {
    fn post(&self, path: &str, body: &Value) -> Result<Answer, String> {
        if let Some(reason) = &self.refused {
            return Err(reason.clone());
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_or_this_computer_is_accepted() {
        for url in [
            "https://licences.example.com",
            "HTTPS://x",
            "http://localhost:8080",
            "http://127.0.0.1",
            "http://[::1]:9/",
        ] {
            assert!(secure_or_local(url), "{url}");
        }
        for url in [
            "http://licences.example.com",
            "http://localhost.example.com",
            "http://127.0.0.1.example.com",
            "ftp://x",
            "",
        ] {
            assert!(!secure_or_local(url), "{url}");
        }
    }

    #[test]
    fn plain_http_elsewhere_sends_nothing() {
        let transport = HttpTransport::new("http://licences.example.com");
        let error = transport.post("/api/v1/activations", &Value::Null).err().unwrap();
        assert!(error.contains("https://"), "{error}");
    }
}
