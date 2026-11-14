// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:42:04.302841
//+ Revised: 	2026/11/14 11:42:04.302841
// -----------------------------------------------------------------------------

//! The messages of licence applications: the form, a product's key page, sending one and asking
//! how it went.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// `GET /api/v1/applications/form`: what the application form offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationFormResponse {
    /// The active products that accept applications, by name.
    pub products: Vec<ApplicationProduct>,
    /// The Cloudflare Turnstile site key the web form shows its check with; `None` without one.
    pub turnstile_site_key: Option<String>,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationProduct {
    pub code: String,
    pub name: String,
    pub description: Option<String>,
}

/// `GET /api/v1/applications/products/{code}`: what a product's key page at
/// `/apply?product=<code>` shows. Only for an active product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyPageResponse {
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    /// How to get a key, in the administrator's words; plain text with line breaks.
    pub key_instructions: Option<String>,
    /// Whether the page also shows the application form.
    pub accepts_applications: bool,
    pub server_time: DateTime<Utc>,
}

/// `POST /api/v1/applications`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationRequest {
    /// The product code.
    pub product: String,
    pub name: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Where it was sent from; an application when missing.
    #[serde(default)]
    pub source: ApplicationSource,
    /// The web form's Cloudflare Turnstile answer, required from the web form when the server has
    /// Turnstile keys; an application sends none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turnstile_token: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationSource {
    /// From inside an application, through the client.
    #[default]
    App,
    /// From the web form at `/apply`.
    Web,
}

impl ApplicationSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Web => "web",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationCreated {
    pub application: Uuid,
    /// 32 random bytes: the only way to ask how the application went. Shown once; the server
    /// keeps only its hash.
    pub secret: String,
    pub server_time: DateTime<Utc>,
}

/// `POST /api/v1/applications/status`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationStatusRequest {
    pub secret: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationStatus {
    /// Waiting for the administrator.
    Pending,
    /// A licence was issued: the answer carries its key.
    Approved,
    /// Turned down, with the administrator's reason when they gave one.
    Rejected,
    /// Approved, but the licence was revoked or deleted since: there is no key to give.
    Withdrawn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationStatusResponse {
    pub application: Uuid,
    /// The product code.
    pub product: String,
    pub status: ApplicationStatus,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    /// Why it was rejected, written by the administrator for the applicant.
    pub reason: Option<String>,
    /// The licence key, formatted, once approved.
    pub key: Option<String>,
    pub licence_expires_at: Option<DateTime<Utc>>,
    pub server_time: DateTime<Utc>,
}
