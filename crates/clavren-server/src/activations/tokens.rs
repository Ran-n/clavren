// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.973586
//+ Revised: 	2026/11/03 04:54:12.973586
// -----------------------------------------------------------------------------

//! Signing activation tokens. Signing key `v<N>` is derived from the master key with a versioned
//! context, so rotating means shipping applications that trust the next key, then raising
//! `CLAVREN_SIGNING_KEY_VERSION`; nothing is stored.

use chrono::{DateTime, Utc};
use clavren_protocol::check::device_hash;
use clavren_protocol::token::{self, Claims};
use ed25519_dalek::{SigningKey, VerifyingKey};
use uuid::Uuid;

use crate::crypto::key_ring::KeyRing;
use crate::http::AppState;

pub fn kid(version: u32) -> String {
    format!("v{version}")
}

pub fn signing_key(keys: &KeyRing, version: u32) -> SigningKey {
    SigningKey::from_bytes(keys.token_signing_seed(version).as_bytes())
}

/// What applications embed to check tokens signed with key `version`.
pub fn public_key(keys: &KeyRing, version: u32) -> VerifyingKey {
    signing_key(keys, version).verifying_key()
}

/// Whom a token is for.
pub struct Subject<'a> {
    pub licence_id: Uuid,
    pub activation_id: Uuid,
    pub product_code: &'a str,
    pub device_public_key: &'a [u8; 32],
    pub licence_expires_at: Option<DateTime<Utc>>,
}

pub struct Issued {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

/// A fresh token: valid for the configured lifetime, or until the licence expires if sooner.
pub fn issue(state: &AppState, subject: &Subject<'_>, now: DateTime<Utc>) -> Issued {
    let lifetime_end = now + state.settings.licensing.token_lifetime;
    let expires_at = subject.licence_expires_at.map_or(lifetime_end, |licence_end| licence_end.min(lifetime_end));
    let claims = Claims {
        iss: state.settings.public_url.clone(),
        aud: subject.product_code.to_string(),
        sub: subject.licence_id,
        act: subject.activation_id,
        dev: device_hash(subject.device_public_key),
        lic_exp: subject.licence_expires_at.map(|at| at.timestamp()),
        iat: now.timestamp(),
        exp: expires_at.timestamp(),
        jti: Uuid::now_v7(),
    };
    let version = state.settings.licensing.signing_key_version;
    Issued { token: token::sign(&claims, &kid(version), &signing_key(&state.keys, version)), expires_at }
}
