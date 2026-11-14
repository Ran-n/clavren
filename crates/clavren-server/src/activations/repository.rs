// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.890784
//+ Revised: 	2026/11/14 11:42:04.186064
// -----------------------------------------------------------------------------

//! Database access for activations.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use super::{Activation, RevocationReason};

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
    /// How long the product's tokens work offline.
    pub product_offline_days: i32,
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
        "SELECT l.id, p.code AS product_code, p.is_active AS product_active, p.offline_days AS product_offline_days,
                l.expires_at, l.revoked_at, l.is_test
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
