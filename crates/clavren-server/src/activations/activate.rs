// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.593310
//+ Revised: 	2026/11/03 04:54:12.593310
// -----------------------------------------------------------------------------

//! Activating a key on a computer: in one transaction, with the licence row locked, so two
//! computers activating the same key at once get one success and one `already_activated`. The
//! same computer retrying (its answer was lost) gets its activation back with a fresh token.

use chrono::{DateTime, Utc};
use clavren_protocol::encoding;
use clavren_protocol::licence_key::LicenceKey;
use clavren_protocol::wire::{ActivationRequest, ErrorCode};
use serde_json::json;
use uuid::Uuid;

use super::repository::{self, KeyedLicence, NewActivation};
use super::tokens::{self, Subject};
use crate::audit::{self, AuditEntry};
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::fields;
use crate::licences::secrets::lookup_hash;

/// The longest device name, platform or version accepted.
pub const MAX_TEXT: usize = 200;

pub struct Activated {
    pub activation_id: Uuid,
    pub token: tokens::Issued,
    pub licence_expires_at: Option<DateTime<Utc>>,
    /// `false` when the same computer asked again and got its existing activation back.
    pub created: bool,
}

/// A request that passed its checks.
struct Valid<'a> {
    key: LicenceKey,
    device_public_key: [u8; 32],
    fingerprint: Option<[u8; 32]>,
    device_name: String,
    platform: String,
    app_version: String,
    product: &'a str,
}

pub async fn activate(
    state: &AppState,
    request: &ActivationRequest,
    ip: std::net::IpAddr,
    now: DateTime<Utc>,
) -> Result<Activated, ApiError> {
    let valid = validate(request)?;
    let mut transaction = state.pool.begin().await?;
    let licence = repository::licence_by_key(&mut transaction, &lookup_hash(&state.keys, &valid.key))
        .await?
        .filter(|licence| licence.product_code == valid.product)
        .ok_or_else(|| ApiError::refused(ErrorCode::InvalidKey, "no licence has this key for this product"))?;
    refuse_unusable(&licence, now)?;
    let subject = |activation_id| Subject {
        licence_id: licence.id,
        activation_id,
        product_code: &licence.product_code,
        device_public_key: &valid.device_public_key,
        licence_expires_at: licence.expires_at,
    };
    if let Some((existing, device)) = repository::active_for_licence(&mut *transaction, licence.id).await? {
        if device != valid.device_public_key {
            return Err(ApiError::refused(ErrorCode::AlreadyActivated, "the licence is in use on another computer"));
        }
        transaction.commit().await?;
        let token = tokens::issue(state, &subject(existing), now);
        return Ok(Activated {
            activation_id: existing,
            token,
            licence_expires_at: licence.expires_at,
            created: false,
        });
    }
    let activation_id = Uuid::now_v7();
    let new = NewActivation {
        id: activation_id,
        licence_id: licence.id,
        device_public_key: &valid.device_public_key,
        device_fingerprint_hash: valid.fingerprint.as_ref().map(<[u8; 32]>::as_slice),
        device_name: &valid.device_name,
        platform: &valid.platform,
        app_version: &valid.app_version,
        at: now,
    };
    repository::insert(&mut *transaction, &new).await?;
    let entry = AuditEntry::by_application("activation.created", "activation", activation_id, ip, now)
        .with_metadata(json!({ "licence": licence.id }));
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    let token = tokens::issue(state, &subject(activation_id), now);
    Ok(Activated { activation_id, token, licence_expires_at: licence.expires_at, created: true })
}

fn refuse_unusable(licence: &KeyedLicence, now: DateTime<Utc>) -> Result<(), ApiError> {
    if licence.revoked_at.is_some() {
        return Err(ApiError::refused(ErrorCode::LicenceRevoked, "the licence was revoked"));
    }
    if !licence.product_active {
        return Err(ApiError::refused(ErrorCode::ProductUnavailable, "the product is no longer available"));
    }
    if licence.expires_at.is_some_and(|at| at <= now) {
        return Err(ApiError::refused(ErrorCode::LicenceExpired, "the licence has expired"));
    }
    Ok(())
}

fn validate(request: &ActivationRequest) -> Result<Valid<'_>, ApiError> {
    let key = LicenceKey::parse(&request.key)
        .ok_or_else(|| ApiError::refused(ErrorCode::InvalidKey, "the key is malformed"))?;
    let device_public_key = encoding::decode_array::<32>(&request.device_public_key)
        .ok_or_else(|| ApiError::invalid_request("device_public_key must be 32 bytes, base64url"))?;
    let fingerprint = match request.device_fingerprint.as_deref() {
        Some(text) => Some(
            encoding::decode_array::<32>(text)
                .ok_or_else(|| ApiError::invalid_request("device_fingerprint must be 32 bytes, base64url"))?,
        ),
        None => None,
    };
    Ok(Valid {
        key,
        device_public_key,
        fingerprint,
        device_name: fields::required("device_name", &request.device_name, MAX_TEXT)?,
        platform: fields::required("platform", &request.platform, MAX_TEXT)?,
        app_version: fields::required("app_version", &request.app_version, MAX_TEXT)?,
        product: request.product.trim(),
    })
}
