// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:48.902546
//+ Revised: 	2026/11/11 17:53:39.444323
// -----------------------------------------------------------------------------

//! Database access for licence applications.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use super::{ApplicationRow, State};
use crate::http::pagination::Cursor;

/// The unique index allowing one waiting application per product and address.
pub const ONE_PENDING_CONSTRAINT: &str = "license_applications_one_pending";

macro_rules! select_applications {
    ($rest:literal) => {
        concat!(
            "SELECT id, product_id, name, email, message, source, created_at, decided_at, decision, license_id,
                    rejection_reason, key_collected_at
             FROM license_applications ",
            $rest
        )
    };
}

pub struct NewApplication<'a> {
    pub id: Uuid,
    pub product_id: Uuid,
    pub name: &'a str,
    pub email: &'a str,
    pub message: Option<&'a str>,
    pub source: &'a str,
    pub secret_hash: &'a [u8; 32],
    pub at: DateTime<Utc>,
}

pub async fn insert(executor: impl PgExecutor<'_>, new: &NewApplication<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO license_applications (id, product_id, name, email, message, source, secret_hash, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(new.id)
    .bind(new.product_id)
    .bind(new.name)
    .bind(new.email)
    .bind(new.message)
    .bind(new.source)
    .bind(new.secret_hash.as_slice())
    .bind(new.at)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn find_many(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<Vec<ApplicationRow>, sqlx::Error> {
    sqlx::query_as(select_applications!("WHERE id = ANY($1) ORDER BY id DESC")).bind(ids).fetch_all(executor).await
}

/// The applications, locked until the transaction ends, in ID order (so two bulk requests over
/// the same rows take their locks in the same order).
pub async fn lock(connection: &mut PgConnection, ids: &[Uuid]) -> Result<Vec<ApplicationRow>, sqlx::Error> {
    sqlx::query_as(select_applications!("WHERE id = ANY($1) ORDER BY id FOR UPDATE"))
        .bind(ids)
        .fetch_all(connection)
        .await
}

pub async fn find_by_secret(
    executor: impl PgExecutor<'_>,
    hash: &[u8; 32],
) -> Result<Option<ApplicationRow>, sqlx::Error> {
    sqlx::query_as(select_applications!("WHERE secret_hash = $1")).bind(hash.as_slice()).fetch_optional(executor).await
}

/// What the list can be narrowed to.
#[derive(Debug, Default)]
pub struct Filter {
    pub state: Option<State>,
    pub product: Option<Uuid>,
    /// An `ILIKE` pattern on the name or the email.
    pub pattern: Option<String>,
}

/// Newest first (IDs are UUIDv7, so ID order is the order they arrived in), after `cursor`.
pub async fn page(
    executor: impl PgExecutor<'_>,
    filter: &Filter,
    cursor: Option<&Cursor>,
    count: i64,
) -> Result<Vec<ApplicationRow>, sqlx::Error> {
    sqlx::query_as(select_applications!(
        "WHERE ($1::text IS NULL OR COALESCE(decision, 'pending') = $1)
           AND ($2::uuid IS NULL OR product_id = $2)
           AND ($3::text IS NULL OR name ILIKE $3 OR email ILIKE $3)
           AND ($4::uuid IS NULL OR id < $4)
         ORDER BY id DESC LIMIT $5"
    ))
    .bind(filter.state.map(State::as_str))
    .bind(filter.product)
    .bind(filter.pattern.as_deref())
    .bind(cursor.map(|cursor| cursor.id))
    .bind(count)
    .fetch_all(executor)
    .await
}

pub async fn approve(
    executor: impl PgExecutor<'_>,
    id: Uuid,
    licence_id: Uuid,
    at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE license_applications SET decided_at = $3, decision = 'approved', license_id = $2 WHERE id = $1",
    )
    .bind(id)
    .bind(licence_id)
    .bind(at)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn reject(
    executor: impl PgExecutor<'_>,
    ids: &[Uuid],
    reason: Option<&str>,
    at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE license_applications SET decided_at = $3, decision = 'rejected', rejection_reason = $2
         WHERE id = ANY($1)",
    )
    .bind(ids)
    .bind(reason)
    .bind(at)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn delete(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM license_applications WHERE id = ANY($1)").bind(ids).execute(executor).await?;
    Ok(())
}

/// Marks the key collected, the first time only; `true` when this was the first time.
pub async fn mark_collected(executor: impl PgExecutor<'_>, id: Uuid, at: DateTime<Utc>) -> Result<bool, sqlx::Error> {
    let updated =
        sqlx::query("UPDATE license_applications SET key_collected_at = $2 WHERE id = $1 AND key_collected_at IS NULL")
            .bind(id)
            .bind(at)
            .execute(executor)
            .await?;
    Ok(updated.rows_affected() == 1)
}

/// Deletes the applications whose licence belongs to `customer`: deleting a customer erases the
/// name and email they applied with too.
pub async fn delete_of_customer(executor: impl PgExecutor<'_>, customer: Uuid) -> Result<u64, sqlx::Error> {
    let deleted = sqlx::query(
        "DELETE FROM license_applications WHERE license_id IN (SELECT id FROM licenses WHERE customer_id = $1)",
    )
    .bind(customer)
    .execute(executor)
    .await?;
    Ok(deleted.rows_affected())
}

pub async fn pending_count(executor: impl PgExecutor<'_>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM license_applications WHERE decided_at IS NULL").fetch_one(executor).await
}

/// The product an application is for, without locking anything.
pub async fn product_of(executor: impl PgExecutor<'_>, id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT product_id FROM license_applications WHERE id = $1")
        .bind(id)
        .fetch_optional(executor)
        .await
}
