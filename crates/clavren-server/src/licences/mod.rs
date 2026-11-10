// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.557866
//+ Revised: 	2026/11/10 17:59:21.176560
// -----------------------------------------------------------------------------

//! Licences: one key for one product, optionally for a customer, optionally until a date. The key
//! is stored encrypted (so it can be shown again) and found by its keyed hash; revoking deletes the
//! encrypted key and keeps the hash, so the key can never be accepted again. A licence's state is
//! derived from its revocation and its active activation, never stored. A test licence is for
//! trying an application: it works on any number of computers, its tokens say it is one, and the
//! Overview leaves it out.

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

pub use issue::issue_test_licence;
pub use lifecycle::{Deleted, delete_locked, end_activation, reset};
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
    /// The latest of its active activations.
    pub active_activation_id: Option<Uuid>,
    /// How many computers it is active on (one at most, unless it is a test licence).
    pub active_computers: i64,
    pub is_test: bool,
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
