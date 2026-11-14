// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.470214
//+ Revised: 	2026/11/14 11:42:04.389697
// -----------------------------------------------------------------------------

//! The public API's requests, responses and error codes (`/api/v1`): activations, checks and
//! licence applications. Binary values are base64url
//! strings; times are RFC 3339 in UTC. Every response carries the server's time, for diagnostics:
//! it isn't signed, so clients take the server's time from the token's `iat` instead.

mod applications;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use applications::*;

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
mod tests;
