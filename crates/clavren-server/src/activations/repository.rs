// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.890784
//+ Revised: 	2026/11/10 17:59:20.352653
// -----------------------------------------------------------------------------

//! Database access for activations.

use chrono::{DateTime, TimeDelta, Utc};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use super::{Activation, RevocationReason};

/// How long the hourly check counts are kept.
pub const CHECK_COUNTS_KEPT: TimeDelta = TimeDelta::days(30);

pub async fn find_many(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<Vec<Activation>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, license_id, device_name, platform, app_version, activated_at, last_checked_at, revoked_at,
                revocation_reason
         FROM activations WHERE id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(executor)
    .await
}

/// Ends every active activation of each licence in `licence_ids`; returns the licence of each
/// activation ended.
pub async fn revoke_active(
    executor: impl PgExecutor<'_>,
    licence_ids: &[Uuid],
    reason: RevocationReason,
    at: DateTime<Utc>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE activations SET revoked_at = $3, revocation_reason = $2
         WHERE license_id = ANY($1) AND revoked_at IS NULL
         RETURNING license_id",
    )
    .bind(licence_ids)
    .bind(reason.as_str())
    .bind(at)
    .fetch_all(executor)
    .await
}

/// A licence found by its key, with what deciding an activation needs.
#[derive(Debug, sqlx::FromRow)]
pub struct KeyedLicence {
    pub id: Uuid,
    pub product_code: String,
    pub product_active: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub is_test: bool,
}

/// The licence whose key hashes to `key_lookup`, locked until the transaction ends, so two
/// computers activating the same key at once are decided one after the other.
pub async fn licence_by_key(
    connection: &mut PgConnection,
    key_lookup: &[u8],
) -> Result<Option<KeyedLicence>, sqlx::Error> {
    sqlx::query_as(
        "SELECT l.id, p.code AS product_code, p.is_active AS product_active, l.expires_at, l.revoked_at, l.is_test
         FROM licenses l JOIN products p ON p.id = l.product_id
         WHERE l.key_lookup = $1 FOR UPDATE OF l",
    )
    .bind(key_lookup)
    .fetch_optional(connection)
    .await
}

/// The licence's active activations (one at most, unless it is a test licence): their IDs and
/// device public keys.
pub async fn active_for_licence(
    executor: impl PgExecutor<'_>,
    licence_id: Uuid,
) -> Result<Vec<(Uuid, Vec<u8>)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, device_public_key FROM activations WHERE license_id = $1 AND revoked_at IS NULL ORDER BY id",
    )
    .bind(licence_id)
    .fetch_all(executor)
    .await
}

pub struct NewActivation<'a> {
    pub id: Uuid,
    pub licence_id: Uuid,
    pub device_public_key: &'a [u8],
    pub device_fingerprint_hash: Option<&'a [u8]>,
    pub device_name: &'a str,
    pub platform: &'a str,
    pub app_version: &'a str,
    pub at: DateTime<Utc>,
}

pub async fn insert(executor: impl PgExecutor<'_>, new: &NewActivation<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO activations (id, license_id, device_public_key, device_fingerprint_hash, device_name, platform,
                                  app_version, activated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(new.id)
    .bind(new.licence_id)
    .bind(new.device_public_key)
    .bind(new.device_fingerprint_hash)
    .bind(new.device_name)
    .bind(new.platform)
    .bind(new.app_version)
    .bind(new.at)
    .execute(executor)
    .await?;
    Ok(())
}

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
}

/// The activation, locked until the transaction ends, so a check and a reset or revoke are
/// decided one after the other.
pub async fn check_target(connection: &mut PgConnection, id: Uuid) -> Result<Option<CheckTarget>, sqlx::Error> {
    sqlx::query_as(
        "SELECT a.id, a.license_id AS licence_id, a.device_public_key, a.revoked_at, a.revocation_reason,
                l.revoked_at AS licence_revoked_at, l.expires_at AS licence_expires_at,
                l.is_test AS licence_is_test, p.code AS product_code,
                p.is_active AS product_active
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

/// Whether the activation exists and is still active.
pub async fn is_active(executor: impl PgExecutor<'_>, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM activations WHERE id = $1 AND revoked_at IS NULL)")
        .bind(id)
        .fetch_one(executor)
        .await
}

/// Ends one activation; returns whether it was still active.
pub async fn revoke_one(
    executor: impl PgExecutor<'_>,
    id: Uuid,
    reason: RevocationReason,
    at: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let ended = sqlx::query(
        "UPDATE activations SET revoked_at = $3, revocation_reason = $2 WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(reason.as_str())
    .bind(at)
    .execute(executor)
    .await?;
    Ok(ended.rows_affected() == 1)
}

/// Whether the activation is one of the licence's active ones.
pub async fn is_active_for(executor: impl PgExecutor<'_>, id: Uuid, licence: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM activations WHERE id = $1 AND license_id = $2 AND revoked_at IS NULL)",
    )
    .bind(id)
    .bind(licence)
    .fetch_one(executor)
    .await
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
