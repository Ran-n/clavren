// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:38:23.801691
//+ Revised: 	2026/11/14 11:38:23.801691
// -----------------------------------------------------------------------------

//! Database writes for licences: inserting them with their encrypted keys, editing, revoking
//! and deleting.

use crate::money::Price;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// Deletes the licences in `ids` with their activations (their challenges and encrypted keys go
/// with them, by cascade). `activations.license_id` is `ON DELETE RESTRICT`, so this is the one
/// place a licence's history is removed.
pub async fn delete(connection: &mut PgConnection, ids: &[Uuid]) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM activations WHERE license_id = ANY($1)").bind(ids).execute(&mut *connection).await?;
    sqlx::query("DELETE FROM licenses WHERE id = ANY($1)").bind(ids).execute(connection).await?;
    Ok(())
}

/// New licences for one product and customer, as parallel lists.
pub struct NewLicences<'a> {
    pub ids: &'a [Uuid],
    pub key_hashes: &'a [Vec<u8>],
    pub product_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub internal_notes: Option<&'a str>,
    pub is_test: bool,
    /// What each key was charged.
    pub price: Option<&'a Price>,
}

/// Inserts in one statement; returns the IDs inserted. A licence whose key hash already exists is
/// skipped, so the caller can give it a new key.
pub async fn insert(executor: impl PgExecutor<'_>, new: &NewLicences<'_>) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO licenses (id, product_id, customer_id, key_lookup, created_at, expires_at, internal_notes, is_test,
                               price_amount, price_currency)
         SELECT id, $3, $4, key_lookup, $5, $6, $7, $8, $9, $10
         FROM unnest($1::uuid[], $2::bytea[]) AS new (id, key_lookup)
         ON CONFLICT (key_lookup) DO NOTHING
         RETURNING id",
    )
    .bind(new.ids)
    .bind(new.key_hashes)
    .bind(new.product_id)
    .bind(new.customer_id)
    .bind(new.created_at)
    .bind(new.expires_at)
    .bind(new.internal_notes)
    .bind(new.is_test)
    .bind(new.price.map(|price| price.amount))
    .bind(new.price.map(|price| price.currency.as_str()))
    .fetch_all(executor)
    .await
}

pub async fn insert_secrets(
    executor: impl PgExecutor<'_>,
    ids: &[Uuid],
    ciphertexts: &[Vec<u8>],
    nonces: &[Vec<u8>],
    version: i16,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO license_key_secrets (license_id, ciphertext, nonce, encryption_version)
         SELECT id, ciphertext, nonce, $4 FROM unnest($1::uuid[], $2::bytea[], $3::bytea[]) AS new (id, ciphertext, nonce)",
    )
    .bind(ids)
    .bind(ciphertexts)
    .bind(nonces)
    .bind(version)
    .execute(executor)
    .await?;
    Ok(())
}

/// What [`update`] changes: per field, `None` leaves it as it is and `Some(None)` clears it.
#[derive(Debug, Default)]
pub struct Changes {
    pub customer_id: Option<Option<Uuid>>,
    pub expires_at: Option<Option<DateTime<Utc>>>,
    pub internal_notes: Option<Option<String>>,
    pub revocation_reason: Option<Option<String>>,
    /// Never applied to a test licence, which has no price.
    pub price: Option<Option<Price>>,
}

/// Which fields of one licence an update changed, in [`Changes`] order: customer, expiry, notes,
/// revocation reason, price.
pub type Changed = (Uuid, bool, bool, bool, bool, bool);

/// Applies `changes` to the licences in `ids` that it changes; the others stay untouched. Returns
/// each changed licence with the fields that changed.
pub async fn update(
    executor: impl PgExecutor<'_>,
    ids: &[Uuid],
    changes: &Changes,
) -> Result<Vec<Changed>, sqlx::Error> {
    sqlx::query_as(
        "WITH old AS (
             SELECT id, customer_id, expires_at, internal_notes, revocation_reason, price_amount, price_currency,
                    is_test
             FROM licenses WHERE id = ANY($1)
         ), updated AS (
             UPDATE licenses AS licence
             SET customer_id = CASE WHEN $2 THEN $3 ELSE licence.customer_id END,
                 expires_at = CASE WHEN $4 THEN $5 ELSE licence.expires_at END,
                 internal_notes = CASE WHEN $6 THEN $7 ELSE licence.internal_notes END,
                 revocation_reason = CASE WHEN $8 THEN $9 ELSE licence.revocation_reason END,
                 price_amount = CASE WHEN $10 AND NOT old.is_test THEN $11 ELSE licence.price_amount END,
                 price_currency = CASE WHEN $10 AND NOT old.is_test THEN $12 ELSE licence.price_currency END
             FROM old
             WHERE licence.id = old.id
               AND (($2 AND old.customer_id IS DISTINCT FROM $3)
                 OR ($4 AND old.expires_at IS DISTINCT FROM $5)
                 OR ($6 AND old.internal_notes IS DISTINCT FROM $7)
                 OR ($8 AND old.revocation_reason IS DISTINCT FROM $9)
                 OR ($10 AND NOT old.is_test
                     AND (old.price_amount, old.price_currency) IS DISTINCT FROM ($11::bigint, $12::text)))
             RETURNING licence.id, licence.customer_id, licence.expires_at, licence.internal_notes,
                       licence.revocation_reason, licence.price_amount, licence.price_currency
         )
         SELECT updated.id,
                updated.customer_id IS DISTINCT FROM old.customer_id,
                updated.expires_at IS DISTINCT FROM old.expires_at,
                updated.internal_notes IS DISTINCT FROM old.internal_notes,
                updated.revocation_reason IS DISTINCT FROM old.revocation_reason,
                (updated.price_amount, updated.price_currency) IS DISTINCT FROM (old.price_amount, old.price_currency)
         FROM updated JOIN old USING (id)",
    )
    .bind(ids)
    .bind(changes.customer_id.is_some())
    .bind(changes.customer_id.flatten())
    .bind(changes.expires_at.is_some())
    .bind(changes.expires_at.flatten())
    .bind(changes.internal_notes.is_some())
    .bind(changes.internal_notes.as_ref().and_then(Option::as_deref))
    .bind(changes.revocation_reason.is_some())
    .bind(changes.revocation_reason.as_ref().and_then(Option::as_deref))
    .bind(changes.price.is_some())
    .bind(changes.price.as_ref().and_then(|price| price.as_ref().map(|price| price.amount)))
    .bind(changes.price.as_ref().and_then(|price| price.as_ref().map(|price| price.currency.as_str())))
    .fetch_all(executor)
    .await
}

/// Revokes every licence in `ids` and deletes their encrypted keys; their lookup hashes stay, so
/// the keys can never be accepted again.
pub async fn revoke(
    connection: &mut PgConnection,
    ids: &[Uuid],
    reason: Option<&str>,
    at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE licenses SET revoked_at = $2, revocation_reason = $3 WHERE id = ANY($1)")
        .bind(ids)
        .bind(at)
        .bind(reason)
        .execute(&mut *connection)
        .await?;
    sqlx::query("DELETE FROM license_key_secrets WHERE license_id = ANY($1)").bind(ids).execute(connection).await?;
    Ok(())
}
