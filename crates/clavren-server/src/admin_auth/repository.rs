// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.314774
//+ Revised: 	2026/11/03 04:54:18.449641
// -----------------------------------------------------------------------------

//! Database access for admin accounts and their recovery codes.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgExecutor};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct AdminRecord {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub totp_secret_ciphertext: Vec<u8>,
    pub totp_secret_nonce: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub disabled_at: Option<DateTime<Utc>>,
}

/// Everything needed to create an admin or replace their credentials.
pub struct Credentials {
    pub password_hash: String,
    pub totp_secret_ciphertext: Vec<u8>,
    pub totp_secret_nonce: Vec<u8>,
    /// The step of the code used to confirm the authenticator, so it can't be reused to log in.
    pub totp_last_used_step: u64,
    pub recovery_code_hashes: Vec<[u8; 32]>,
}

/// `SELECT <every AdminRecord column> FROM admin_users <rest>`, as one literal: sqlx only accepts
/// SQL fixed at compile time.
macro_rules! select_admins {
    ($rest:literal) => {
        concat!(
            "SELECT id, username, password_hash, totp_secret_ciphertext, totp_secret_nonce, created_at, last_login_at, ",
            "disabled_at FROM admin_users ",
            $rest
        )
    };
}

pub async fn find_by_username(
    executor: impl PgExecutor<'_>,
    username: &str,
) -> Result<Option<AdminRecord>, sqlx::Error> {
    sqlx::query_as(select_admins!("WHERE lower(username) = lower($1)")).bind(username).fetch_optional(executor).await
}

pub async fn find_by_id(executor: impl PgExecutor<'_>, id: Uuid) -> Result<Option<AdminRecord>, sqlx::Error> {
    sqlx::query_as(select_admins!("WHERE id = $1")).bind(id).fetch_optional(executor).await
}

pub async fn list(executor: impl PgExecutor<'_>) -> Result<Vec<AdminRecord>, sqlx::Error> {
    sqlx::query_as(select_admins!("ORDER BY lower(username)")).fetch_all(executor).await
}

/// Inserts the account; its recovery codes are stored with [`replace_recovery_codes`].
pub async fn insert(
    executor: impl PgExecutor<'_>,
    id: Uuid,
    username: &str,
    credentials: &Credentials,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO admin_users (id, username, password_hash, totp_secret_ciphertext, totp_secret_nonce,
                                  totp_last_used_step, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(username)
    .bind(&credentials.password_hash)
    .bind(&credentials.totp_secret_ciphertext)
    .bind(&credentials.totp_secret_nonce)
    .bind(step(credentials.totp_last_used_step))
    .bind(now)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn set_password(executor: impl PgExecutor<'_>, id: Uuid, password_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE admin_users SET password_hash = $2 WHERE id = $1")
        .bind(id)
        .bind(password_hash)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn set_totp(
    executor: impl PgExecutor<'_>,
    id: Uuid,
    ciphertext: &[u8],
    nonce: &[u8],
    last_used_step: u64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE admin_users SET totp_secret_ciphertext = $2, totp_secret_nonce = $3, totp_last_used_step = $4
         WHERE id = $1",
    )
    .bind(id)
    .bind(ciphertext)
    .bind(nonce)
    .bind(step(last_used_step))
    .execute(executor)
    .await?;
    Ok(())
}

/// Records `step` as used, if it is newer than the last one used. `false` means the code (or a
/// later one) was already used: a replay.
pub async fn claim_totp_step(executor: impl PgExecutor<'_>, id: Uuid, step_used: u64) -> Result<bool, sqlx::Error> {
    let claimed = sqlx::query(
        "UPDATE admin_users SET totp_last_used_step = $2
         WHERE id = $1 AND (totp_last_used_step IS NULL OR totp_last_used_step < $2)",
    )
    .bind(id)
    .bind(step(step_used))
    .execute(executor)
    .await?;
    Ok(claimed.rows_affected() == 1)
}

/// Marks the recovery code with this hash as used, if it is one of the admin's unused codes.
pub async fn use_recovery_code(
    executor: impl PgExecutor<'_>,
    admin_id: Uuid,
    code_hash: &[u8; 32],
    now: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let used = sqlx::query(
        "UPDATE admin_recovery_codes SET used_at = $3
         WHERE admin_user_id = $1 AND code_hash = $2 AND used_at IS NULL",
    )
    .bind(admin_id)
    .bind(code_hash.as_slice())
    .bind(now)
    .execute(executor)
    .await?;
    Ok(used.rows_affected() == 1)
}

pub async fn unused_recovery_codes(executor: impl PgExecutor<'_>, admin_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM admin_recovery_codes WHERE admin_user_id = $1 AND used_at IS NULL")
        .bind(admin_id)
        .fetch_one(executor)
        .await
}

/// Deletes the admin's codes and stores `hashes` instead. Run in a transaction.
pub async fn replace_recovery_codes(
    transaction: &mut sqlx::PgConnection,
    admin_id: Uuid,
    hashes: &[[u8; 32]],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM admin_recovery_codes WHERE admin_user_id = $1")
        .bind(admin_id)
        .execute(&mut *transaction)
        .await?;
    for hash in hashes {
        sqlx::query("INSERT INTO admin_recovery_codes (id, admin_user_id, code_hash) VALUES ($1, $2, $3)")
            .bind(Uuid::now_v7())
            .bind(admin_id)
            .bind(hash.as_slice())
            .execute(&mut *transaction)
            .await?;
    }
    Ok(())
}

pub async fn record_login(executor: impl PgExecutor<'_>, id: Uuid, now: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE admin_users SET last_login_at = $2 WHERE id = $1").bind(id).bind(now).execute(executor).await?;
    Ok(())
}

/// Sets or clears `disabled_at`; returns whether anything changed.
pub async fn set_disabled(
    executor: impl PgExecutor<'_>,
    id: Uuid,
    disabled_at: Option<DateTime<Utc>>,
) -> Result<bool, sqlx::Error> {
    let changed = sqlx::query(
        "UPDATE admin_users SET disabled_at = $2 WHERE id = $1 AND (disabled_at IS NULL) <> ($2::timestamptz IS NULL)",
    )
    .bind(id)
    .bind(disabled_at)
    .execute(executor)
    .await?;
    Ok(changed.rows_affected() == 1)
}

/// TOTP steps fit easily: the current one is about 59 million.
fn step(step: u64) -> i64 {
    i64::try_from(step).unwrap_or(i64::MAX)
}
