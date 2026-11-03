// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:34.955979
//+ Revised: 	2026/11/03 04:54:17.988271
// -----------------------------------------------------------------------------

//! Checking an admin's second factor: a code from the authenticator app (each usable once), or one
//! of their one-time recovery codes. Shared by login and the account endpoints that ask for it.

use chrono::{DateTime, Utc};

use super::repository::{self, AdminRecord};
use super::{recovery_codes, totp};
use crate::http::AppState;
use crate::http::error::ApiError;

pub async fn second_factor_matches(
    state: &AppState,
    admin: &AdminRecord,
    code: &str,
    now: DateTime<Utc>,
) -> Result<bool, ApiError> {
    let code = code.trim();
    if code.len() == 6 && code.bytes().all(|byte| byte.is_ascii_digit()) {
        return authenticator_code_matches(state, admin, code, now).await;
    }
    let Some(canonical) = recovery_codes::normalize(code) else { return Ok(false) };
    let used = repository::use_recovery_code(&state.pool, admin.id, &recovery_codes::hash(&canonical), now).await?;
    if used {
        let left = repository::unused_recovery_codes(&state.pool, admin.id).await?;
        tracing::warn!(admin = %admin.username, left, "a recovery code was used");
    }
    Ok(used)
}

async fn authenticator_code_matches(
    state: &AppState,
    admin: &AdminRecord,
    code: &str,
    now: DateTime<Utc>,
) -> Result<bool, ApiError> {
    let secret = totp::open(&state.keys, admin.id, &admin.totp_secret_ciphertext, &admin.totp_secret_nonce)
        .map_err(ApiError::internal)?;
    let Some(step) = totp::matching_step(&secret, &admin.username, code, now) else { return Ok(false) };
    let fresh = repository::claim_totp_step(&state.pool, admin.id, step).await?;
    if !fresh {
        tracing::warn!(admin = %admin.username, "an authenticator code was used a second time");
    }
    Ok(fresh)
}
