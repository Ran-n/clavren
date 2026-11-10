// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:34.824500
//+ Revised: 	2026/11/09 22:30:54.140807
// -----------------------------------------------------------------------------

//! What a signed-in admin can change about their own account: password, recovery codes and
//! authenticator app. Each asks for the password again; failed attempts count towards the login
//! rate limit, so a stolen session can't be used to guess the password.

use std::net::IpAddr;

use serde::Deserialize;
use serde_json::json;

use super::factors::second_factor_matches;
use super::guard::SignedIn;
use super::repository::{self, AdminRecord};
use super::{passwords, recovery_codes, sessions, totp};
use crate::audit::{self, AuditEntry};
use crate::http::AppState;
use crate::http::error::ApiError;

#[derive(Deserialize)]
pub struct ChangePassword {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct ConfirmIdentity {
    pub password: String,
    pub code: String,
}

#[derive(Deserialize)]
pub struct ReplaceAuthenticator {
    pub password: String,
    /// The Base32 secret from [`new_authenticator_secret`].
    pub secret: String,
    /// A code from the app, set up with the new secret.
    pub code: String,
}

pub struct NewAuthenticatorSecret {
    pub secret: String,
    pub uri: String,
}

/// Sets a new password and signs the admin out everywhere else.
pub async fn change_password(
    state: &AppState,
    signed_in: &SignedIn,
    request: ChangePassword,
    ip: IpAddr,
) -> Result<(), ApiError> {
    let admin = confirm_password(state, signed_in, request.current_password, ip).await?;
    passwords::validate_new(&request.new_password).map_err(|error| ApiError::weak_password(error.to_string()))?;
    let hash = passwords::hash(request.new_password).await.map_err(ApiError::internal)?;
    let now = state.clock.now();
    let mut transaction = state.pool.begin().await?;
    repository::set_password(&mut *transaction, admin.id, &hash).await?;
    let others = sessions::revoke_all(&mut *transaction, admin.id, Some(signed_in.session.session_id), now).await?;
    let entry = AuditEntry::admin_account("admin.password_changed", Some(admin.id), Some(admin.id), Some(ip), now)
        .with_metadata(json!({ "sessions_revoked": others }));
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    tracing::info!(admin = %admin.username, sessions_revoked = others, "admin changed their password");
    Ok(())
}

/// Replaces all recovery codes; returns the new ones (shown once).
pub async fn regenerate_recovery_codes(
    state: &AppState,
    signed_in: &SignedIn,
    request: ConfirmIdentity,
    ip: IpAddr,
) -> Result<Vec<String>, ApiError> {
    let admin = confirm_password(state, signed_in, request.password, ip).await?;
    let now = state.clock.now();
    if !second_factor_matches(state, &admin, &request.code, now).await? {
        state.login_failures.record_failure(&username_key(&admin), now);
        return Err(ApiError::invalid_credentials());
    }
    let codes = recovery_codes::generate().map_err(ApiError::internal)?;
    let hashes = recovery_codes::hash_all(&codes)
        .ok_or_else(|| ApiError::internal("a new recovery code doesn't read back as one"))?;
    let mut transaction = state.pool.begin().await?;
    repository::replace_recovery_codes(&mut transaction, admin.id, &hashes).await?;
    let entry =
        AuditEntry::admin_account("admin.recovery_codes_regenerated", Some(admin.id), Some(admin.id), Some(ip), now);
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    tracing::info!(admin = %admin.username, "admin regenerated their recovery codes");
    Ok(codes)
}

/// A fresh secret to set up in the authenticator app. Nothing is stored until
/// [`replace_authenticator`] confirms a code made with it.
pub fn new_authenticator_secret(signed_in: &SignedIn) -> Result<NewAuthenticatorSecret, ApiError> {
    let secret = totp::new_secret().map_err(ApiError::internal)?;
    let uri = totp::setup_uri(&secret, &signed_in.session.username).map_err(ApiError::internal)?;
    Ok(NewAuthenticatorSecret { secret: totp::secret_text(&secret), uri })
}

pub async fn replace_authenticator(
    state: &AppState,
    signed_in: &SignedIn,
    request: ReplaceAuthenticator,
    ip: IpAddr,
) -> Result<(), ApiError> {
    let admin = confirm_password(state, signed_in, request.password, ip).await?;
    let secret = totp::secret_from_text(&request.secret)
        .ok_or_else(|| ApiError::invalid_request("the secret is not one Clavren issued"))?;
    let now = state.clock.now();
    let step = totp::matching_step(&secret, &admin.username, request.code.trim(), now)
        .ok_or_else(|| ApiError::invalid_request("the code doesn't match the new secret; check the app's clock"))?;
    let sealed = totp::seal(&state.keys, admin.id, &secret).map_err(ApiError::internal)?;
    let mut transaction = state.pool.begin().await?;
    repository::set_totp(&mut *transaction, admin.id, &sealed.ciphertext, &sealed.nonce, step).await?;
    let entry = AuditEntry::admin_account("admin.totp_changed", Some(admin.id), Some(admin.id), Some(ip), now);
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    tracing::info!(admin = %admin.username, "admin set up a new authenticator app");
    Ok(())
}

async fn confirm_password(
    state: &AppState,
    signed_in: &SignedIn,
    password: String,
    ip: IpAddr,
) -> Result<AdminRecord, ApiError> {
    let admin = repository::find_by_id(&state.pool, signed_in.session.admin_user_id)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    let key = username_key(&admin);
    let now = state.clock.now();
    // counted first, as at sign-in, and taken back when the password is right
    state.login_failures.attempt(std::slice::from_ref(&key), now).map_err(ApiError::rate_limited)?;
    if passwords::verify(password, admin.password_hash.clone()).await.map_err(ApiError::internal)? {
        state.login_failures.forgive(&key, now);
        return Ok(admin);
    }
    tracing::warn!(admin = %admin.username, %ip, "wrong password when changing account settings");
    Err(ApiError::invalid_credentials())
}

/// The same key login uses, so both kinds of failure share one limit.
fn username_key(admin: &AdminRecord) -> String {
    super::login::username_key(&admin.username)
}
