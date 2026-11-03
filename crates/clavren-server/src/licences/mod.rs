// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.557866
//+ Revised: 	2026/11/03 04:54:15.557866
// -----------------------------------------------------------------------------

//! Licences: one key for one product, optionally for a customer, optionally until a date. The key
//! is stored encrypted (so it can be shown again) and found by its keyed hash; revoking deletes the
//! encrypted key and keeps the hash, so the key can never be accepted again. A licence's state is
//! derived from its revocation and its active activation, never stored.

mod disclosure;
mod filter;
mod issue;
mod lifecycle;
pub mod repository;
mod routes;
pub mod secrets;
mod views;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

pub use lifecycle::reset;
pub use routes::routes;
pub use views::{LicenceView, present};

/// Bulk requests take up to this many licences at once.
pub const MAX_BATCH: usize = 1000;
const MAX_NOTES: usize = 2000;
const MAX_REASON: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Unused,
    Redeemed,
    Revoked,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unused => "unused",
            Self::Redeemed => "redeemed",
            Self::Revoked => "revoked",
        }
    }
}

/// A licence as the `license_states` view sees it.
#[derive(Debug, Clone, FromRow)]
pub struct LicenceRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub state: String,
    pub active_activation_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revocation_reason: Option<String>,
    pub internal_notes: Option<String>,
}

impl LicenceRow {
    pub fn state(&self) -> State {
        match self.state.as_str() {
            "revoked" => State::Revoked,
            "redeemed" => State::Redeemed,
            _ => State::Unused,
        }
    }
}
