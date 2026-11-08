// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.624141
//+ Revised: 	2026/11/07 19:36:59.803436
// -----------------------------------------------------------------------------

//! Database access for licences and their encrypted keys.

use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use sqlx::postgres::PgArguments;
use sqlx::query::QueryAs;
use sqlx::{FromRow, PgConnection, PgExecutor, Postgres};
use uuid::Uuid;

use super::LicenceRow;
use super::filter::{Criteria, Filter, Order};
use crate::http::pagination::Cursor;

/// Every [`LicenceRow`] column.
const SELECT_LICENCES: &str =
    "SELECT l.id, l.product_id, l.customer_id, s.state, s.active_activation_id, l.created_at, l.expires_at,
            l.revoked_at, l.revocation_reason, l.internal_notes
     FROM licenses l
     JOIN license_states s ON s.license_id = l.id
     LEFT JOIN customers c ON c.id = l.customer_id";

/// The parameter holding `now` in a [`page`] query, after the two sets of criteria ($1 to $14).
const NOW: usize = 15;

/// What each [`Criteria`] field matches, from its first parameter `$n` on: the condition, and the
/// parameters that leave it unset when all are `NULL`.
fn matches(n: usize) -> [(String, Vec<usize>); 5] {
    let (product, customer, state, expiry, key, id, pattern) = (n, n + 1, n + 2, n + 3, n + 4, n + 5, n + 6);
    [
        (format!("l.product_id = ${product}::uuid"), vec![product]),
        (format!("l.customer_id = ${customer}::uuid"), vec![customer]),
        (format!("s.state = ${state}::text"), vec![state]),
        (
            format!(
                "(${expiry}::text = 'permanent' AND l.expires_at IS NULL)
                 OR (${expiry} = 'expiring' AND l.expires_at > ${NOW}::timestamptz
                     AND l.expires_at <= ${NOW} + interval '30 days')
                 OR (${expiry} = 'expired' AND l.expires_at <= ${NOW})"
            ),
            vec![expiry],
        ),
        (
            format!(
                "l.key_lookup = ${key}::bytea OR l.id = ${id}::uuid
                 OR c.name ILIKE ${pattern}::text OR c.email ILIKE ${pattern}"
            ),
            vec![key, id, pattern],
        ),
    ]
}

/// The `WHERE` condition of a [`Filter`]: every `only` criterion ($1 to $7) holds, and no `except`
/// one ($8 to $14) does. An exclusion is `NOT COALESCE(…, false)` so a licence the criterion can't
/// say anything about (no customer, no expiry) isn't hidden by it.
fn filter_condition() -> String {
    let only = matches(1).map(|(condition, params)| {
        let unset = params.iter().map(|n| format!("${n} IS NULL")).collect::<Vec<_>>().join(" AND ");
        format!("(({unset}) OR {condition})")
    });
    let except = matches(8).map(|(condition, _)| format!("NOT COALESCE({condition}, false)"));
    only.into_iter().chain(except).collect::<Vec<_>>().join("\n AND ")
}

/// A [`page`] query in each order; the cursor is $16 (key) and $17 (ID), the count $18.
static NEWEST: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{SELECT_LICENCES} WHERE {} AND ($17::uuid IS NULL OR l.id < $17) ORDER BY l.id DESC LIMIT $18",
        filter_condition()
    )
});
static SOONEST_EXPIRY: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{SELECT_LICENCES} WHERE {}
           AND ($17::uuid IS NULL OR (COALESCE(l.expires_at, 'infinity'), l.id) > ($16::timestamptz, $17))
         ORDER BY COALESCE(l.expires_at, 'infinity'), l.id LIMIT $18",
        filter_condition()
    )
});

fn bind_criteria<'q, O>(
    query: QueryAs<'q, Postgres, O, PgArguments>,
    criteria: &'q Criteria,
) -> QueryAs<'q, Postgres, O, PgArguments> {
    query
        .bind(criteria.product)
        .bind(criteria.customer)
        .bind(criteria.state)
        .bind(criteria.expiry)
        .bind(criteria.key_hash.as_deref())
        .bind(criteria.licence_id)
        .bind(criteria.pattern.as_deref())
}

/// The licences matching `filter`, in `order`, after `cursor`.
pub async fn page(
    executor: impl PgExecutor<'_>,
    filter: &Filter,
    order: Order,
    cursor: Option<&Cursor>,
    count: i64,
) -> Result<Vec<LicenceRow>, sqlx::Error> {
    let sql = match order {
        Order::Newest => NEWEST.as_str(),
        Order::Expiry => SOONEST_EXPIRY.as_str(),
    };
    bind_criteria(bind_criteria(sqlx::query_as(sql), &filter.only), &filter.except)
        .bind(filter.now)
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

static FIND_MANY: LazyLock<String> =
    LazyLock::new(|| format!("{SELECT_LICENCES} WHERE l.id = ANY($1) ORDER BY l.id DESC"));

pub async fn find_many(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<Vec<LicenceRow>, sqlx::Error> {
    sqlx::query_as(FIND_MANY.as_str()).bind(ids).fetch_all(executor).await
}

/// The state of each licence in `ids` that exists, as `(id, state)`.
pub async fn states(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as("SELECT license_id, state FROM license_states WHERE license_id = ANY($1) ORDER BY license_id")
        .bind(ids)
        .fetch_all(executor)
        .await
}

/// Deletes the licences in `ids` with their activations (their challenges and encrypted keys go
/// with them, by cascade). `activations.license_id` is `ON DELETE RESTRICT`, so this is the one
/// place a licence's history is removed.
pub async fn delete(connection: &mut PgConnection, ids: &[Uuid]) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM activations WHERE license_id = ANY($1)").bind(ids).execute(&mut *connection).await?;
    sqlx::query("DELETE FROM licenses WHERE id = ANY($1)").bind(ids).execute(connection).await?;
    Ok(())
}

/// Every licence of the product, locked until the transaction ends.
pub async fn ids_of_product(connection: &mut PgConnection, product_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM licenses WHERE product_id = $1 ORDER BY id FOR UPDATE")
        .bind(product_id)
        .fetch_all(connection)
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

/// What [`update`] changes: per field, `None` leaves it as it is and `Some(None)` clears it.
#[derive(Debug, Default)]
pub struct Changes {
    pub customer_id: Option<Option<Uuid>>,
    pub expires_at: Option<Option<DateTime<Utc>>>,
    pub internal_notes: Option<Option<String>>,
    pub revocation_reason: Option<Option<String>>,
}

/// Applies `changes` to every licence in `ids`.
pub async fn update(executor: impl PgExecutor<'_>, ids: &[Uuid], changes: &Changes) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE licenses SET customer_id = CASE WHEN $2 THEN $3 ELSE customer_id END,
                             expires_at = CASE WHEN $4 THEN $5 ELSE expires_at END,
                             internal_notes = CASE WHEN $6 THEN $7 ELSE internal_notes END,
                             revocation_reason = CASE WHEN $8 THEN $9 ELSE revocation_reason END
         WHERE id = ANY($1)",
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
