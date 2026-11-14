// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:42:04.082282
//+ Revised: 	2026/11/14 11:42:04.082282
// -----------------------------------------------------------------------------

//! Database access for checks: their one-use challenges, the activation a check is about,
//! recording a passed check, and the daily counts the Overview shows.

use chrono::{DateTime, TimeDelta, Utc};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// How long the hourly check counts are kept.
pub const CHECK_COUNTS_KEPT: TimeDelta = TimeDelta::days(30);

/// An activation with its licence and product, as a check sees them.
#[derive(Debug, sqlx::FromRow)]
pub struct CheckTarget {
    pub id: Uuid,
    pub licence_id: Uuid,
    pub device_public_key: Vec<u8>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revocation_reason: Option<String>,
    pub licence_revoked_at: Option<DateTime<Utc>>,
    pub licence_expires_at: Option<DateTime<Utc>>,
    pub licence_is_test: bool,
    pub product_code: String,
    pub product_active: bool,
    /// How long the product's tokens work offline.
    pub product_offline_days: i32,
}

/// The activation, locked until the transaction ends, so a check and a reset or revoke are
/// decided one after the other.
pub async fn check_target(connection: &mut PgConnection, id: Uuid) -> Result<Option<CheckTarget>, sqlx::Error> {
    sqlx::query_as(
        "SELECT a.id, a.license_id AS licence_id, a.device_public_key, a.revoked_at, a.revocation_reason,
                l.revoked_at AS licence_revoked_at, l.expires_at AS licence_expires_at,
                l.is_test AS licence_is_test, p.code AS product_code,
                p.is_active AS product_active, p.offline_days AS product_offline_days
         FROM activations a JOIN licenses l ON l.id = a.license_id JOIN products p ON p.id = l.product_id
         WHERE a.id = $1 FOR UPDATE OF a",
    )
    .bind(id)
    .fetch_optional(connection)
    .await
}

pub async fn record_check(
    executor: impl PgExecutor<'_>,
    id: Uuid,
    app_version: &str,
    at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE activations SET last_checked_at = $3, app_version = $2 WHERE id = $1")
        .bind(id)
        .bind(app_version)
        .bind(at)
        .execute(executor)
        .await?;
    Ok(())
}

/// Adds one successful check to the count for the hour of `at`.
pub async fn count_check(executor: impl PgExecutor<'_>, at: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO check_counts (hour, checks) VALUES (date_trunc('hour', $1), 1)
         ON CONFLICT (hour) DO UPDATE SET checks = check_counts.checks + 1",
    )
    .bind(at)
    .execute(executor)
    .await?;
    Ok(())
}

/// Deletes the counts of hours that ended `CHECK_COUNTS_KEPT` ago or earlier.
pub async fn delete_old_check_counts(executor: impl PgExecutor<'_>, now: DateTime<Utc>) -> Result<u64, sqlx::Error> {
    let result =
        sqlx::query("DELETE FROM check_counts WHERE hour < $1").bind(now - CHECK_COUNTS_KEPT).execute(executor).await?;
    Ok(result.rows_affected())
}

pub async fn insert_challenge(
    executor: impl PgExecutor<'_>,
    id: Uuid,
    activation_id: Uuid,
    nonce_hash: &[u8],
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO verification_challenges (id, activation_id, nonce_hash, created_at, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(activation_id)
    .bind(nonce_hash)
    .bind(created_at)
    .bind(expires_at)
    .execute(executor)
    .await?;
    Ok(())
}

/// Marks the challenge used, if it belongs to the activation, is unused and hasn't expired;
/// returns its nonce hash. One statement, so two checks can't both use it.
pub async fn consume_challenge(
    executor: impl PgExecutor<'_>,
    id: Uuid,
    activation_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE verification_challenges SET consumed_at = $3
         WHERE id = $1 AND activation_id = $2 AND consumed_at IS NULL AND expires_at > $3
         RETURNING nonce_hash",
    )
    .bind(id)
    .bind(activation_id)
    .bind(now)
    .fetch_optional(executor)
    .await
}

/// Deletes challenges that were used or expired; returns how many.
pub async fn delete_finished_challenges(executor: impl PgExecutor<'_>, now: DateTime<Utc>) -> Result<u64, sqlx::Error> {
    let deleted = sqlx::query("DELETE FROM verification_challenges WHERE consumed_at IS NOT NULL OR expires_at <= $1")
        .bind(now)
        .execute(executor)
        .await?;
    Ok(deleted.rows_affected())
}
