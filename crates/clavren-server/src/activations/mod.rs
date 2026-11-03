// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.770875
//+ Revised: 	2026/11/03 04:54:12.770875
// -----------------------------------------------------------------------------

//! Activations: the computers (Windows user accounts) a licence was bound to. A licence has at most
//! one active activation; revoked ones stay as history, with the reason.

pub mod activate;
mod admin_routes;
pub mod checks;
pub mod public_routes;
pub mod repository;
pub mod tokens;

pub use admin_routes::routes;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// What the admin API shows of an activation. The device's public key and fingerprint stay
/// inside the server.
#[derive(Debug, Clone, PartialEq, Serialize, FromRow)]
pub struct Activation {
    pub id: Uuid,
    #[serde(rename = "licence")]
    pub license_id: Uuid,
    pub device_name: String,
    pub platform: String,
    pub app_version: String,
    pub activated_at: DateTime<Utc>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    /// `reset` (the licence was freed for another computer) or `licence_revoked`.
    pub revocation_reason: Option<String>,
}

/// Why an activation stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationReason {
    Reset,
    LicenceRevoked,
}

impl RevocationReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reset => "reset",
            Self::LicenceRevoked => "licence_revoked",
        }
    }
}
