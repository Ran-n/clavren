// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.624141
//+ Revised: 	2026/11/03 04:54:15.624141
// -----------------------------------------------------------------------------

//! Database access for licences and their encrypted keys.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgConnection, PgExecutor};
use uuid::Uuid;

use super::LicenceRow;
use super::filter::{Filter, Order};
use crate::http::pagination::Cursor;

/// Every [`LicenceRow`] column, with the filters every list and export applies ($1 to $8).
macro_rules! select_licences {
    ($rest:literal) => {
        concat!(
            "SELECT l.id, l.product_id, l.customer_id, s.state, s.active_activation_id, l.created_at, l.expires_at,
                    l.revoked_at, l.revocation_reason, l.internal_notes
             FROM licenses l
             JOIN license_states s ON s.license_id = l.id
             LEFT JOIN customers c ON c.id = l.customer_id
             WHERE ($1::uuid IS NULL OR l.product_id = $1)
               AND ($2::uuid IS NULL OR l.customer_id = $2)
               AND ($3::text IS NULL OR s.state = $3)
               AND ($4::text IS NULL
                    OR ($4 = 'permanent' AND l.expires_at IS NULL)
                    OR ($4 = 'expiring' AND l.expires_at > $5 AND l.expires_at <= $5 + interval '30 days')
                    OR ($4 = 'expired' AND l.expires_at <= $5))
               AND (($6::bytea IS NULL AND $7::uuid IS NULL AND $8::text IS NULL)
                    OR l.key_lookup = $6 OR l.id = $7 OR c.name ILIKE $8 OR c.email ILIKE $8) ",
            $rest
        )
    };
}

/// The licences matching `filter`, in `order`, after `cursor`.
pub async fn page(
    executor: impl PgExecutor<'_>,
    filter: &Filter,
    order: Order,
    cursor: Option<&Cursor>,
    count: i64,
) -> Result<Vec<LicenceRow>, sqlx::Error> {
    let query = match order {
        Order::Newest => {
            sqlx::query_as(select_licences!("AND ($10::uuid IS NULL OR l.id < $10) ORDER BY l.id DESC LIMIT $11"))
        }
        Order::Expiry => sqlx::query_as(select_licences!(
            "AND ($10::uuid IS NULL OR (COALESCE(l.expires_at, 'infinity'), l.id) > ($9::timestamptz, $10))
             ORDER BY COALESCE(l.expires_at, 'infinity'), l.id LIMIT $11"
        )),
    };
    query
        .bind(filter.product)
        .bind(filter.customer)
        .bind(filter.state)
        .bind(filter.expiry)
        .bind(filter.now)
        .bind(filter.key_hash.as_deref())
        .bind(filter.licence_id)
        .bind(filter.pattern.as_deref())
        .bind(cursor.and_then(|cursor| cursor.key.clone()))
        .bind(cursor.map(|cursor| cursor.id))
        .bind(count)
        .fetch_all(executor)
        .await
}

/// Where a page in `order` ending at `licence` continues.
pub fn cursor_after(order: Order) -> impl Fn(&LicenceRow) -> Cursor {
    move |licence| match order {
        Order::Newest => Cursor { key: None, id: licence.id },
        Order::Expiry => Cursor {
            key: Some(licence.expires_at.map_or_else(|| "infinity".to_string(), |at| at.to_rfc3339())),
            id: licence.id,
        },
    }
}

pub fn cursor_fits(order: Order, cursor: &Cursor) -> bool {
    match order {
        Order::Newest => cursor.key.is_none(),
        Order::Expiry => {
            cursor.key.as_deref().is_some_and(|key| key == "infinity" || DateTime::parse_from_rfc3339(key).is_ok())
        }
    }
}

pub async fn find_many(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<Vec<LicenceRow>, sqlx::Error> {
    sqlx::query_as(select_licences!("AND l.id = ANY($9) ORDER BY l.id DESC"))
        .bind(None::<Uuid>)
        .bind(None::<Uuid>)
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(None::<DateTime<Utc>>)
        .bind(None::<Vec<u8>>)
        .bind(None::<Uuid>)
        .bind(None::<String>)
        .bind(ids)
        .fetch_all(executor)
        .await
}

/// Locks the licences in `ids` until the transaction ends; returns `(id, revoked)` for those that
/// exist.
pub async fn lock(connection: &mut PgConnection, ids: &[Uuid]) -> Result<Vec<(Uuid, bool)>, sqlx::Error> {
    sqlx::query_as("SELECT id, revoked_at IS NOT NULL FROM licenses WHERE id = ANY($1) ORDER BY id FOR UPDATE")
        .bind(ids)
        .fetch_all(connection)
        .await
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
}

/// Inserts in one statement; returns the IDs inserted. A licence whose key hash already exists is
/// skipped, so the caller can give it a new key.
pub async fn insert(executor: impl PgExecutor<'_>, new: &NewLicences<'_>) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO licenses (id, product_id, customer_id, key_lookup, created_at, expires_at, internal_notes)
         SELECT id, $3, $4, key_lookup, $5, $6, $7 FROM unnest($1::uuid[], $2::bytea[]) AS new (id, key_lookup)
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

/// A licence's encrypted key, with what opening it needs.
#[derive(Debug, FromRow)]
pub struct StoredKey {
    pub license_id: Uuid,
    pub product_id: Uuid,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

/// The encrypted keys of `ids`; revoked licences have none.
pub async fn stored_keys(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<Vec<StoredKey>, sqlx::Error> {
    sqlx::query_as(
        "SELECT k.license_id, l.product_id, k.ciphertext, k.nonce
         FROM license_key_secrets k JOIN licenses l ON l.id = k.license_id
         WHERE k.license_id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(executor)
    .await
}

/// Sets the expiry and/or notes of every licence in `ids`; `None` leaves a field as it is.
pub async fn update(
    executor: impl PgExecutor<'_>,
    ids: &[Uuid],
    expires_at: Option<Option<DateTime<Utc>>>,
    internal_notes: Option<Option<&str>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE licenses SET expires_at = CASE WHEN $2 THEN $3 ELSE expires_at END,
                             internal_notes = CASE WHEN $4 THEN $5 ELSE internal_notes END
         WHERE id = ANY($1)",
    )
    .bind(ids)
    .bind(expires_at.is_some())
    .bind(expires_at.flatten())
    .bind(internal_notes.is_some())
    .bind(internal_notes.flatten())
    .execute(executor)
    .await?;
    Ok(())
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
