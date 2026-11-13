// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:48.731360
//+ Revised: 	2026/11/11 17:53:38.638338
// -----------------------------------------------------------------------------

//! Licence applications: someone asks for a licence for a product that accepts them, from inside
//! the application (through the client) or from the web form at `/apply`. The administrator
//! approves one, which issues its licence, or rejects it with an optional reason. The applicant
//! keeps a random secret and asks with it how it went; an approved application answers with the
//! key, so the application activates it by itself and the web form's status page shows it. Only
//! the secret's hash is stored. Decided applications are kept as history until the administrator
//! deletes them.

mod admin_routes;
mod decide;
pub mod human_check;
mod public_routes;
pub mod repository;
mod submit;
mod views;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

pub use admin_routes::routes;
pub use public_routes::routes as public_routes;

pub const MAX_NAME: usize = 200;
pub const MAX_MESSAGE: usize = 2000;
pub const MAX_REASON: usize = 500;

/// Where an application stands, as the dashboard sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Pending,
    Approved,
    Rejected,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }
}

/// An application as stored, without its secret's hash.
#[derive(Debug, Clone, FromRow)]
pub struct ApplicationRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub name: String,
    pub email: String,
    pub message: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decision: Option<String>,
    pub license_id: Option<Uuid>,
    pub rejection_reason: Option<String>,
    pub key_collected_at: Option<DateTime<Utc>>,
}

impl ApplicationRow {
    pub fn state(&self) -> State {
        match self.decision.as_deref() {
            Some("approved") => State::Approved,
            Some(_) => State::Rejected,
            None => State::Pending,
        }
    }
}
