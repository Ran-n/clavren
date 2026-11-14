// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.963172
//+ Revised: 	2026/11/14 11:42:03.988263
// -----------------------------------------------------------------------------

//! Products: the applications that use Clavren. A product's `code` is the audience of every token
//! issued for it, so it can't change once the product has licences. Deactivating a product pauses
//! it and keeps everything; deleting it deletes its licences and their history too, and the
//! computers using them lock at their next check, and its licence applications go too. A product
//! takes licence applications only when the administrator opens them. Its page at
//! `/apply?product=<code>` says how to get a key, in the administrator's words. Without a code, one is made from the name. A new
//! product comes with one test licence, unless the request asks for none.

mod automatic_code;
mod checks;
mod input;
pub mod message_template;
pub mod repository;
mod routes;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::audit::{self, Actor, AuditEntry};
use crate::http::error::ApiError;
use crate::licences::{self, Deleted};
use crate::money::PriceColumns;
use checks::*;

pub use input::{NewProduct, ProductChanges};
pub use routes::routes;

/// The fewest offline days a product may have: applications check once a day, so 1 would lock a
/// computer after a single missed check (a laptop shut over a weekend).
pub const MIN_OFFLINE_DAYS: i32 = 2;
/// The most: ten years, so the limit is always a real one. There is deliberately no "forever": an
/// application that is never online would never learn its licence was revoked or reset.
pub const MAX_OFFLINE_DAYS: i32 = 3650;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Product {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub message_template: String,
    pub is_active: bool,
    /// Whether people may apply for a licence of it (in the application or on the web form).
    pub accepts_applications: bool,
    /// How to get a key, shown on the product's page at `/apply?product=<code>`.
    pub key_instructions: Option<String>,
    /// How many days its applications keep working without reaching the server: every token of
    /// its licences lasts this long (capped at the licence's expiry).
    pub offline_days: i32,
    /// Its price per key, which issuing fills in for each new licence; `null`: none.
    #[sqlx(flatten)]
    pub price: PriceColumns,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// How many licences a product has, and how many of them are in use on a computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, FromRow)]
pub struct Usage {
    pub licences: i64,
    pub in_use: i64,
}

/// Deletes `product` (locked by the caller's transaction) with every licence it has, their keys
/// and their history, auditing each licence and the product. Set-based: no limit on how many.
pub async fn delete_with_licences(
    connection: &mut PgConnection,
    actor: &Actor,
    product: &Product,
) -> Result<Deleted, ApiError> {
    let ids = licences::repository::ids_of_product(&mut *connection, product.id).await?;
    let deleted = licences::delete_locked(&mut *connection, actor, &ids, false).await?;
    repository::delete(&mut *connection, product.id).await?;
    let metadata = json!({ "licences": deleted.deleted, "in_use": deleted.in_use });
    audit::record(
        &mut *connection,
        AuditEntry::by(actor, "product.deleted", "product", product.id).with_metadata(metadata),
    )
    .await?;
    Ok(deleted)
}

#[cfg(test)]
mod tests;
