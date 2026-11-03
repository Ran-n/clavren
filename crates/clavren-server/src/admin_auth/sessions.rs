// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.434303
//+ Revised: 	2026/11/03 04:54:18.587564
// -----------------------------------------------------------------------------

//! Admin sessions: a 256-bit random token in a cookie, of which only the BLAKE3 hash is stored.
//! A session ends after a period without use, at its fixed lifetime, at logout, or when revoked.
//! Its CSRF token is derived from the session token, so it needs no storage of its own.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgExecutor};
use uuid::Uuid;

use crate::config::AdminAuthSettings;
use crate::crypto::key_ring::KeyRing;
use crate::crypto::random::{RandomError, random_bytes};

const TOKEN_BYTES: usize = 32;
/// Writing `last_seen_at` on every request would turn every read into a write.
pub const TOUCH_INTERVAL_SECONDS: i64 = 60;

pub struct NewSession {
    pub id: Uuid,
    /// Goes into the cookie; never stored.
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ActiveSession {
    pub session_id: Uuid,
    pub admin_user_id: Uuid,
    pub username: String,
    pub expires_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

pub fn token_hash(token: &str) -> [u8; 32] {
    *blake3::hash(token.as_bytes()).as_bytes()
}

pub fn csrf_token(keys: &KeyRing, session_token: &str) -> String {
    URL_SAFE_NO_PAD.encode(blake3::keyed_hash(keys.csrf().as_bytes(), session_token.as_bytes()).as_bytes())
}

/// Compares in constant time, through `blake3::Hash`'s equality.
pub fn csrf_matches(keys: &KeyRing, session_token: &str, provided: &str) -> bool {
    let expected = blake3::keyed_hash(keys.csrf().as_bytes(), session_token.as_bytes());
    let Ok(bytes) = URL_SAFE_NO_PAD.decode(provided.trim()) else { return false };
    <[u8; 32]>::try_from(bytes.as_slice()).is_ok_and(|bytes| blake3::Hash::from_bytes(bytes) == expected)
}

pub async fn create(
    executor: impl PgExecutor<'_>,
    admin_id: Uuid,
    now: DateTime<Utc>,
    settings: &AdminAuthSettings,
) -> Result<NewSession, SessionError> {
    let token = URL_SAFE_NO_PAD.encode(random_bytes::<TOKEN_BYTES>()?);
    let expires_at = now + settings.session_lifetime;
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO admin_sessions (id, admin_user_id, token_hash, created_at, last_seen_at, expires_at)
         VALUES ($1, $2, $3, $4, $4, $5)",
    )
    .bind(id)
    .bind(admin_id)
    .bind(token_hash(&token).as_slice())
    .bind(now)
    .bind(expires_at)
    .execute(executor)
    .await?;
    Ok(NewSession { id, token, expires_at })
}

/// The session for `token`, if it is still valid and its admin is enabled.
pub async fn find_active(
    executor: impl PgExecutor<'_>,
    token: &str,
    now: DateTime<Utc>,
    settings: &AdminAuthSettings,
) -> Result<Option<ActiveSession>, sqlx::Error> {
    sqlx::query_as(
        "SELECT s.id AS session_id, s.admin_user_id, a.username, s.expires_at, s.last_seen_at
         FROM admin_sessions s JOIN admin_users a ON a.id = s.admin_user_id
         WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > $2 AND s.last_seen_at > $3
           AND a.disabled_at IS NULL",
    )
    .bind(token_hash(token).as_slice())
    .bind(now)
    .bind(now - settings.session_idle)
    .fetch_optional(executor)
    .await
}

pub async fn touch(executor: impl PgExecutor<'_>, session_id: Uuid, now: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE admin_sessions SET last_seen_at = $2 WHERE id = $1")
        .bind(session_id)
        .bind(now)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn revoke(executor: impl PgExecutor<'_>, session_id: Uuid, now: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE admin_sessions SET revoked_at = $2 WHERE id = $1 AND revoked_at IS NULL")
        .bind(session_id)
        .bind(now)
        .execute(executor)
        .await?;
    Ok(())
}

/// Revokes every live session of the admin except `keep`; returns how many.
pub async fn revoke_all(
    executor: impl PgExecutor<'_>,
    admin_id: Uuid,
    keep: Option<Uuid>,
    now: DateTime<Utc>,
) -> Result<u64, sqlx::Error> {
    let revoked = sqlx::query(
        "UPDATE admin_sessions SET revoked_at = $3
         WHERE admin_user_id = $1 AND revoked_at IS NULL AND ($2::uuid IS NULL OR id <> $2)",
    )
    .bind(admin_id)
    .bind(keep)
    .bind(now)
    .execute(executor)
    .await?;
    Ok(revoked.rows_affected())
}

/// Deletes sessions that can never be used again; returns how many.
pub async fn delete_finished(
    executor: impl PgExecutor<'_>,
    now: DateTime<Utc>,
    settings: &AdminAuthSettings,
) -> Result<u64, sqlx::Error> {
    let deleted = sqlx::query(
        "DELETE FROM admin_sessions WHERE revoked_at IS NOT NULL OR expires_at <= $1 OR last_seen_at <= $2",
    )
    .bind(now)
    .bind(now - settings.session_idle)
    .execute(executor)
    .await?;
    Ok(deleted.rows_affected())
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Random(#[from] RandomError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::master_key::MasterKey;

    fn keys(byte: u8) -> KeyRing {
        KeyRing::new(MasterKey::from_bytes([byte; 32]))
    }

    #[test]
    fn the_csrf_token_matches_its_own_session_only() {
        let keys = keys(1);
        let token = csrf_token(&keys, "session-a");
        assert!(csrf_matches(&keys, "session-a", &token));
        assert!(!csrf_matches(&keys, "session-b", &token));
        assert!(!csrf_matches(&self::keys(2), "session-a", &token), "another master key");
    }

    #[test]
    fn garbled_csrf_tokens_never_match() {
        let keys = keys(1);
        for provided in ["", "not base64!", "AAAA", &"A".repeat(43)] {
            assert!(!csrf_matches(&keys, "session-a", provided), "{provided:?}");
        }
    }

    #[test]
    fn token_hashes_differ_per_token() {
        assert_ne!(token_hash("a"), token_hash("b"));
        assert_eq!(token_hash("a"), *blake3::hash(b"a").as_bytes());
    }
}
