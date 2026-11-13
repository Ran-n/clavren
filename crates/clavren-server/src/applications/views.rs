// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:49.071634
//+ Revised: 	2026/11/11 16:32:53.796998
// -----------------------------------------------------------------------------

//! What the admin API shows of an application. The product and the licence it was approved with
//! are IDs unless expanded (`product`, `licence`, and the licence's own fields as
//! `licence.customer`…); a page is expanded with one query per field. The secret never leaves the
//! server, and neither does its hash.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use super::{ApplicationRow, State};
use crate::http::expand::{Expand, Expandable, by_id, distinct};
use crate::licences::{self, LicenceView};
use crate::products::{self, Product};

pub const EXPANDABLE: [&str; 5] =
    ["product", "licence", "licence.product", "licence.customer", "licence.active_activation"];

#[derive(Debug, Clone, Serialize)]
pub struct ApplicationView {
    pub id: Uuid,
    pub product: Expandable<Product>,
    pub name: String,
    pub email: String,
    pub message: Option<String>,
    /// `app` or `web`.
    pub source: String,
    pub state: State,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    /// The licence the approval issued; `None` until approved, and once that licence is deleted.
    pub licence: Option<Expandable<LicenceView>>,
    pub rejection_reason: Option<String>,
    /// When the applicant first collected the key.
    pub key_collected_at: Option<DateTime<Utc>>,
}

pub async fn present(
    pool: &PgPool,
    rows: Vec<ApplicationRow>,
    expand: &Expand,
) -> Result<Vec<ApplicationView>, sqlx::Error> {
    let products = if expand.has("product") {
        by_id(products::repository::find_many(pool, &distinct(&rows, |row| Some(row.product_id))).await?, |p| p.id)
    } else {
        HashMap::new()
    };
    let licences = if expand.has("licence") {
        let found = licences::repository::find_many(pool, &distinct(&rows, |row| row.license_id)).await?;
        by_id(licences::present(pool, found, &expand.nested("licence")).await?, |licence| licence.id)
    } else {
        HashMap::new()
    };
    Ok(rows
        .into_iter()
        .map(|row| ApplicationView {
            id: row.id,
            product: Expandable::of(row.product_id, &products),
            state: row.state(),
            licence: row.license_id.map(|id| Expandable::of(id, &licences)),
            name: row.name,
            email: row.email,
            message: row.message,
            source: row.source,
            created_at: row.created_at,
            decided_at: row.decided_at,
            rejection_reason: row.rejection_reason,
            key_collected_at: row.key_collected_at,
        })
        .collect())
}
