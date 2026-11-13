// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:48.633583
//+ Revised: 	2026/11/11 16:32:53.351919
// -----------------------------------------------------------------------------

//! The web form's human check: Cloudflare Turnstile. The page shows Cloudflare's widget, which
//! hands it a one-time answer; the server asks Cloudflare whether that answer is good before
//! taking the application. Behind a trait, so tests answer without the network.

use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::time::Duration;

use serde::Deserialize;

use crate::config::{TURNSTILE_SECRET_VAR as SECRET_VAR, TurnstileSettings};

/// Turnstile answers are at most this long.
pub const MAX_ANSWER: usize = 2048;
const TIMEOUT: Duration = Duration::from_secs(10);

pub type Verdict<'a> = Pin<Box<dyn Future<Output = Result<bool, String>> + Send + 'a>>;

pub trait HumanCheck: Send + Sync {
    /// `Ok(true)` when `answer` proves a person sent the form from `ip`, `Ok(false)` when it
    /// doesn't, `Err` when it couldn't be decided (Cloudflare can't be reached).
    fn verify<'a>(&'a self, answer: &'a str, ip: IpAddr) -> Verdict<'a>;
}

/// Asks Cloudflare's `siteverify` endpoint.
pub struct Turnstile {
    agent: ureq::Agent,
    settings: TurnstileSettings,
}

impl Turnstile {
    pub fn new(settings: TurnstileSettings) -> Self {
        let config = ureq::Agent::config_builder().timeout_global(Some(TIMEOUT)).http_status_as_error(false).build();
        Self { agent: config.into(), settings }
    }
}

#[derive(Deserialize)]
struct Answer {
    success: bool,
    #[serde(default, rename = "error-codes")]
    error_codes: Vec<String>,
}

impl HumanCheck for Turnstile {
    fn verify<'a>(&'a self, answer: &'a str, ip: IpAddr) -> Verdict<'a> {
        let (agent, settings, answer) = (self.agent.clone(), self.settings.clone(), answer.to_string());
        Box::pin(async move {
            let asked = tokio::task::spawn_blocking(move || {
                let form = [("secret", settings.secret.as_str()), ("response", &answer), ("remoteip", &ip.to_string())];
                let mut response =
                    agent.post(&settings.verify_url).send_form(form).map_err(|error| error.to_string())?;
                let status = response.status().as_u16();
                let text = response.body_mut().read_to_string().map_err(|error| error.to_string())?;
                if status != 200 {
                    return Err(format!("Turnstile answered HTTP {status}"));
                }
                serde_json::from_str::<Answer>(&text)
                    .map_err(|error| format!("Turnstile's answer is malformed: {error}"))
            })
            .await
            .map_err(|error| error.to_string())??;
            // a wrong secret is the server's fault, not the applicant's
            if asked.error_codes.iter().any(|code| code.ends_with("-input-secret")) {
                return Err(format!("Turnstile refused the secret ({:?}): check {}", asked.error_codes, SECRET_VAR));
            }
            if !asked.success {
                tracing::warn!(codes = ?asked.error_codes, "Turnstile refused the web form's answer");
            }
            Ok(asked.success)
        })
    }
}
