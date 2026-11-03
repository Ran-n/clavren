// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.712018
//+ Revised: 	2026/11/03 08:25:52.451043
// -----------------------------------------------------------------------------

//! The daily check: the application asks for a challenge, signs it with its device key, and gets a
//! fresh token if its licence is still good. A challenge works once, for two minutes, and only for
//! the activation it was made for; it is used up before the signature is even looked at, so a
//! refused attempt can't be replayed.

use std::net::IpAddr;

use chrono::{DateTime, Duration, Utc};
use clavren_protocol::check::{NONCE_LENGTH, check_message};
use clavren_protocol::encoding;
use clavren_protocol::wire::{CheckRequest, ErrorCode};
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::json;
use uuid::Uuid;

use super::activate::MAX_TEXT;
use super::repository::{self, CheckTarget};
use super::tokens::{self, Subject};
use crate::audit::{self, AuditEntry};
use crate::crypto::random::random_bytes;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::fields;

pub const CHALLENGE_LIFETIME: Duration = Duration::minutes(2);

pub struct Challenge {
    pub id: Uuid,
    pub nonce: [u8; NONCE_LENGTH],
    pub expires_at: DateTime<Utc>,
}

pub async fn challenge(state: &AppState, activation_id: Uuid, now: DateTime<Utc>) -> Result<Challenge, ApiError> {
    if !repository::is_active(&state.pool, activation_id).await? {
        return Err(ApiError::refused(ErrorCode::ActivationRevoked, "this computer's activation has ended"));
    }
    let nonce = random_bytes::<NONCE_LENGTH>().map_err(ApiError::internal)?;
    let challenge = Challenge { id: Uuid::now_v7(), nonce, expires_at: now + CHALLENGE_LIFETIME };
    repository::insert_challenge(
        &state.pool,
        challenge.id,
        activation_id,
        blake3::hash(&nonce).as_bytes(),
        now,
        challenge.expires_at,
    )
    .await?;
    Ok(challenge)
}

pub struct Checked {
    pub token: tokens::Issued,
    pub licence_expires_at: Option<DateTime<Utc>>,
}

pub async fn check(
    state: &AppState,
    activation_id: Uuid,
    request: &CheckRequest,
    ip: IpAddr,
    now: DateTime<Utc>,
) -> Result<Checked, ApiError> {
    let app_version = fields::required("app_version", &request.app_version, MAX_TEXT)?;
    let invalid =
        || ApiError::refused(ErrorCode::ChallengeInvalid, "the challenge is unknown, used, expired or wrongly signed");
    let nonce_hash =
        repository::consume_challenge(&state.pool, request.challenge, activation_id, now).await?.ok_or_else(invalid)?;
    let nonce = encoding::decode_array::<NONCE_LENGTH>(&request.nonce).ok_or_else(invalid)?;
    if blake3::hash(&nonce).as_bytes().as_slice() != nonce_hash.as_slice() {
        return Err(invalid());
    }
    let mut transaction = state.pool.begin().await?;
    let target = repository::check_target(&mut transaction, activation_id).await?.ok_or_else(invalid)?;
    let device_public_key: [u8; 32] = target.device_public_key.as_slice().try_into().map_err(|_| invalid())?;
    if !signature_matches(
        &device_public_key,
        &check_message(activation_id, request.challenge, &nonce),
        &request.signature,
    ) {
        return Err(invalid());
    }
    if let Err(refusal) = refuse_unusable(&target, now) {
        let entry = AuditEntry::by_application("check.refused", "activation", activation_id, ip, now)
            .with_metadata(json!({ "reason": refusal.code() }));
        audit::record(&mut *transaction, entry).await?;
        transaction.commit().await?;
        return Err(refusal);
    }
    repository::record_check(&mut *transaction, activation_id, &app_version, now).await?;
    repository::count_check(&mut *transaction, now).await?;
    transaction.commit().await?;
    let subject = Subject {
        licence_id: target.licence_id,
        activation_id,
        product_code: &target.product_code,
        device_public_key: &device_public_key,
        licence_expires_at: target.licence_expires_at,
    };
    Ok(Checked { token: tokens::issue(state, &subject, now), licence_expires_at: target.licence_expires_at })
}

fn signature_matches(device_public_key: &[u8; 32], message: &str, signature: &str) -> bool {
    let (Ok(key), Some(signature)) =
        (VerifyingKey::from_bytes(device_public_key), encoding::decode_array::<64>(signature))
    else {
        return false;
    };
    key.verify_strict(message.as_bytes(), &Signature::from_bytes(&signature)).is_ok()
}

/// Why this computer may no longer use the licence, if it may not. A revoked licence also ended
/// its activation; the application is told the licence was revoked, not just this computer.
fn refuse_unusable(target: &CheckTarget, now: DateTime<Utc>) -> Result<(), ApiError> {
    if target.licence_revoked_at.is_some() {
        return Err(ApiError::refused(ErrorCode::LicenceRevoked, "the licence was revoked"));
    }
    if target.revoked_at.is_some() {
        let reason = target.revocation_reason.as_deref().unwrap_or("revoked");
        return Err(ApiError::refused(
            ErrorCode::ActivationRevoked,
            format!("this computer's activation ended ({reason})"),
        ));
    }
    if !target.product_active {
        return Err(ApiError::refused(ErrorCode::ProductUnavailable, "the product is no longer available"));
    }
    if target.licence_expires_at.is_some_and(|at| at <= now) {
        return Err(ApiError::refused(ErrorCode::LicenceExpired, "the licence has expired"));
    }
    Ok(())
}
