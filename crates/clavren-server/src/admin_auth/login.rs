// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.077667
//+ Revised: 	2026/11/08 14:55:44.159696
// -----------------------------------------------------------------------------

//! Admin login: password and second factor in one request, rate limited per client address and
//! per username, with one answer for every kind of failure.

use std::net::IpAddr;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::factors::second_factor_matches;
use super::passwords;
use super::repository::{self, AdminRecord};
use super::sessions::{self, NewSession};
use crate::audit::{self, AuditEntry};
use crate::http::AppState;
use crate::http::client_ip::limit_key;
use crate::http::error::ApiError;

const MAX_USERNAME_LENGTH: usize = 64;
const MAX_CODE_LENGTH: usize = 64;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub code: String,
}

pub struct SignedInAdmin {
    pub id: Uuid,
    pub username: String,
    pub session: NewSession,
}

/// Why a login failed, for the audit log and the server log only; the client never learns it.
enum Failure {
    UnknownUser,
    Password(AdminRecord),
    SecondFactor(AdminRecord),
    Disabled(AdminRecord),
}

pub async fn login(state: &AppState, request: LoginRequest, ip: IpAddr) -> Result<SignedInAdmin, ApiError> {
    validate(&request)?;
    let now = state.clock.now();
    // one spelling of the username for the lookup and the limit: `alice ` is `alice`
    let username = request.username.trim();
    let limit_keys = [limit_key(ip), username_key(username)];
    // counted before the password is checked, so parallel guesses can't all slip under the limit
    state.login_failures.attempt(&limit_keys, now).map_err(|wait| {
        tracing::warn!(%ip, "login refused: too many failed attempts");
        ApiError::rate_limited(wait)
    })?;
    let outcome = authenticate(state, &request, username, now).await;
    if !matches!(outcome, Ok(Err(_))) {
        // signed in, or the server failed: neither is a wrong guess
        state.login_failures.forgive(&limit_keys[0], now);
        state.login_failures.forgive(&limit_keys[1], now);
    }
    match outcome? {
        Ok(admin) => succeed(state, admin, ip, now, &limit_keys[1]).await,
        Err(failure) => {
            fail(state, failure, ip, now).await?;
            Err(ApiError::invalid_credentials())
        }
    }
}

/// The limit key for a username, shared by sign-in and account changes.
pub fn username_key(username: &str) -> String {
    format!("user:{}", username.trim().to_lowercase())
}

fn validate(request: &LoginRequest) -> Result<(), ApiError> {
    let too_long = request.username.chars().count() > MAX_USERNAME_LENGTH
        || request.password.chars().count() > passwords::MAX_LENGTH
        || request.code.chars().count() > MAX_CODE_LENGTH;
    if too_long || request.username.trim().is_empty() {
        return Err(ApiError::invalid_request("username, password and code are required, within their length limits"));
    }
    Ok(())
}

async fn authenticate(
    state: &AppState,
    request: &LoginRequest,
    username: &str,
    now: DateTime<Utc>,
) -> Result<Result<AdminRecord, Failure>, ApiError> {
    let Some(admin) = repository::find_by_username(&state.pool, username).await? else {
        passwords::verify_against_nothing(request.password.clone()).await;
        return Ok(Err(Failure::UnknownUser));
    };
    if !passwords::verify(request.password.clone(), admin.password_hash.clone()).await {
        return Ok(Err(Failure::Password(admin)));
    }
    if admin.disabled_at.is_some() {
        return Ok(Err(Failure::Disabled(admin)));
    }
    if !second_factor_matches(state, &admin, &request.code, now).await? {
        return Ok(Err(Failure::SecondFactor(admin)));
    }
    Ok(Ok(admin))
}

async fn succeed(
    state: &AppState,
    admin: AdminRecord,
    ip: IpAddr,
    now: DateTime<Utc>,
    username_key: &str,
) -> Result<SignedInAdmin, ApiError> {
    state.login_failures.clear(username_key);
    let mut transaction = state.pool.begin().await?;
    let session =
        sessions::create(&mut *transaction, admin.id, now, &state.settings.admin).await.map_err(ApiError::internal)?;
    repository::record_login(&mut *transaction, admin.id, now).await?;
    let entry = AuditEntry::admin_account("admin.login", Some(admin.id), Some(admin.id), Some(ip), now);
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    tracing::info!(admin = %admin.username, %ip, "admin signed in");
    Ok(SignedInAdmin { id: admin.id, username: admin.username, session })
}

async fn fail(state: &AppState, failure: Failure, ip: IpAddr, now: DateTime<Utc>) -> Result<(), ApiError> {
    // the typed username is only ever recorded when it names a real admin: people sometimes type
    // their password into the username field
    let (reason, admin) = match &failure {
        Failure::UnknownUser => ("unknown_user", None),
        Failure::Password(admin) => ("password", Some(admin)),
        Failure::SecondFactor(admin) => ("second_factor", Some(admin)),
        Failure::Disabled(admin) => ("disabled", Some(admin)),
    };
    match admin {
        Some(admin) => tracing::warn!(admin = %admin.username, %ip, reason, "admin login failed"),
        None => tracing::warn!(%ip, reason, "admin login failed"),
    }
    let entry = AuditEntry::admin_account("admin.login_failed", None, admin.map(|admin| admin.id), Some(ip), now)
        .with_metadata(json!({ "reason": reason }));
    audit::record(&state.pool, entry).await?;
    Ok(())
}
