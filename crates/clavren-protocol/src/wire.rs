// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.470214
//+ Revised: 	2026/11/12 23:57:43.717027
// -----------------------------------------------------------------------------

//! The public API's requests, responses and error codes (`/api/v1`): activations, checks and
//! licence applications. Binary values are base64url
//! strings; times are RFC 3339 in UTC. Every response carries the server's time, for diagnostics:
//! it isn't signed, so clients take the server's time from the token's `iat` instead.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// `POST /api/v1/activations`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationRequest {
    /// The product code.
    pub product: String,
    /// The licence key as the customer typed it.
    pub key: String,
    /// The device's Ed25519 public key (32 bytes).
    pub device_public_key: String,
    /// A BLAKE3 hash of hardware identifiers (32 bytes), for diagnostics only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_fingerprint: Option<String>,
    pub device_name: String,
    pub platform: String,
    pub app_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivationResponse {
    pub activation: Uuid,
    pub token: String,
    pub token_expires_at: DateTime<Utc>,
    pub licence_expires_at: Option<DateTime<Utc>>,
    pub server_time: DateTime<Utc>,
}

/// `POST /api/v1/activations/{activation}/challenges` (no body)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengeResponse {
    pub challenge: Uuid,
    /// 32 random bytes to sign, valid for 2 minutes and once.
    pub nonce: String,
    pub expires_at: DateTime<Utc>,
    pub server_time: DateTime<Utc>,
}

/// `POST /api/v1/activations/{activation}/checks`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRequest {
    pub challenge: Uuid,
    /// The nonce from the challenge, sent back (the server keeps only its hash).
    pub nonce: String,
    /// Ed25519 signature (64 bytes) of [`crate::check::check_message`].
    pub signature: String,
    pub app_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResponse {
    pub token: String,
    pub token_expires_at: DateTime<Utc>,
    pub licence_expires_at: Option<DateTime<Utc>>,
    pub server_time: DateTime<Utc>,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_time: Option<DateTime<Utc>>,
}

/// Every error code of the public API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Malformed, unknown, or for another product: the server never says which.
    InvalidKey,
    /// In use on another computer.
    AlreadyActivated,
    LicenceRevoked,
    LicenceExpired,
    /// The product was deactivated.
    ProductUnavailable,
    /// This computer was reset or revoked, or the activation doesn't exist.
    ActivationRevoked,
    /// Unknown, expired, already used or foreign challenge, or a wrong signature.
    ChallengeInvalid,
    /// This address already has an application waiting for this product.
    ApplicationPending,
    /// No application has this secret (wrong, or deleted since).
    ApplicationUnknown,
    /// The web form's human check (Cloudflare Turnstile) failed.
    HumanCheckFailed,
    RateLimited,
    InvalidRequest,
    InternalError,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidKey => "invalid_key",
            Self::AlreadyActivated => "already_activated",
            Self::LicenceRevoked => "licence_revoked",
            Self::LicenceExpired => "licence_expired",
            Self::ProductUnavailable => "product_unavailable",
            Self::ActivationRevoked => "activation_revoked",
            Self::ChallengeInvalid => "challenge_invalid",
            Self::ApplicationPending => "application_pending",
            Self::ApplicationUnknown => "application_unknown",
            Self::HumanCheckFailed => "human_check_failed",
            Self::RateLimited => "rate_limited",
            Self::InvalidRequest => "invalid_request",
            Self::InternalError => "internal_error",
        }
    }

    /// The code for `text`, if it is one.
    pub fn parse(text: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(text.to_string())).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_read_back_as_written() {
        for code in [
            ErrorCode::InvalidKey,
            ErrorCode::AlreadyActivated,
            ErrorCode::LicenceRevoked,
            ErrorCode::LicenceExpired,
            ErrorCode::ProductUnavailable,
            ErrorCode::ActivationRevoked,
            ErrorCode::ChallengeInvalid,
            ErrorCode::ApplicationPending,
            ErrorCode::ApplicationUnknown,
            ErrorCode::HumanCheckFailed,
            ErrorCode::RateLimited,
            ErrorCode::InvalidRequest,
            ErrorCode::InternalError,
        ] {
            assert_eq!(ErrorCode::parse(code.as_str()), Some(code));
            assert_eq!(serde_json::to_value(code).unwrap(), code.as_str());
        }
        assert_eq!(ErrorCode::parse("teapot"), None);
    }
}
