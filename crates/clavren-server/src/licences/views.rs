// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.810047
//+ Revised: 	2026/11/12 23:57:45.120859
// -----------------------------------------------------------------------------

//! What the admin API shows of a licence. Related records are IDs unless expanded
//! (`product`, `customer`, `active_activation`); a page is expanded with one query per field.
//! The key itself only appears where it is asked for: issuing, reveal, message and export.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use super::{LicenceRow, State};
use crate::activations::{self, Activation};
use crate::customers::{self, Customer};
use crate::http::expand::{Expand, Expandable, by_id, distinct};
use crate::money::Price;
use crate::products::{self, Product};

pub const EXPANDABLE: [&str; 3] = ["product", "customer", "active_activation"];

#[derive(Debug, Clone, Serialize)]
pub struct LicenceView {
    pub id: Uuid,
    pub product: Expandable<Product>,
    pub customer: Option<Expandable<Customer>>,
    pub state: State,
    /// The latest computer it is active on; a test licence can be on several.
    pub active_activation: Option<Expandable<Activation>>,
    /// How many computers it is active on.
    pub active_computers: i64,
    pub is_test: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revocation_reason: Option<String>,
    pub internal_notes: Option<String>,
    /// What its key was charged; `null`: not recorded (always on a test licence).
    pub price: Option<Price>,
}

/// The views of `rows`, with the fields `expand` asks for loaded.
pub async fn present(pool: &PgPool, rows: Vec<LicenceRow>, expand: &Expand) -> Result<Vec<LicenceView>, sqlx::Error> {
    let products = if expand.has("product") {
        by_id(products::repository::find_many(pool, &distinct(&rows, |row| Some(row.product_id))).await?, |p| p.id)
    } else {
        HashMap::new()
    };
    let customers = if expand.has("customer") {
        by_id(customers::repository::find_many(pool, &distinct(&rows, |row| row.customer_id)).await?, |c| c.id)
    } else {
        HashMap::new()
    };
    let activations = if expand.has("active_activation") {
        let ids = distinct(&rows, |row| row.active_activation_id);
        by_id(activations::repository::find_many(pool, &ids).await?, |a| a.id)
    } else {
        HashMap::new()
    };
    Ok(rows
        .into_iter()
        .map(|row| LicenceView {
            id: row.id,
            product: Expandable::of(row.product_id, &products),
            customer: row.customer_id.map(|id| Expandable::of(id, &customers)),
            state: row.state(),
            active_activation: row.active_activation_id.map(|id| Expandable::of(id, &activations)),
            active_computers: row.active_computers,
            is_test: row.is_test,
            created_at: row.created_at,
            expires_at: row.expires_at,
            revoked_at: row.revoked_at,
            revocation_reason: row.revocation_reason,
            price: row.price.price(),
            internal_notes: row.internal_notes,
        })
        .collect())
}
