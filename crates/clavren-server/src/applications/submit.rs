// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:48.988604
//+ Revised: 	2026/11/11 16:32:53.675374
// -----------------------------------------------------------------------------

//! The applicant's side: sending an application, and asking with its secret how it went. An
//! approved application answers with its licence key, as long as the licence still has one (it
//! wasn't revoked or deleted); the first answer with the key is audited.

use std::net::IpAddr;

use chrono::{DateTime, Utc};
use clavren_protocol::encoding;
use clavren_protocol::wire::{ApplicationRequest, ApplicationSource, ApplicationStatus, ErrorCode};
use serde_json::json;
use uuid::Uuid;

use super::human_check::MAX_ANSWER;
use super::repository::{self, NewApplication, ONE_PENDING_CONSTRAINT};
use super::{ApplicationRow, MAX_MESSAGE, MAX_NAME, State};
use crate::audit::{self, AuditEntry};
use crate::crypto::random::random_bytes;
use crate::database::violates_unique;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::fields;
use crate::licences::{self, secrets};
use crate::products;

const ENTITY: &str = "application";

pub struct Sent {
    pub id: Uuid,
    /// The secret, base64url: shown to the applicant once.
    pub secret: String,
}

/// Takes an application: checked, the web form's answer verified, stored with its secret's hash.
pub async fn submit(
    state: &AppState,
    request: &ApplicationRequest,
    ip: IpAddr,
    now: DateTime<Utc>,
) -> Result<Sent, ApiError> {
    let name = fields::required("name", &request.name, MAX_NAME)?;
    let email = fields::email(Some(&request.email))?.ok_or_else(|| ApiError::invalid_request("email is required"))?;
    let message = fields::optional("message", request.message.as_deref(), MAX_MESSAGE)?;
    let product = products::repository::find_by_code(&state.pool, request.product.trim())
        .await?
        .filter(|product| product.is_active && product.accepts_applications)
        .ok_or_else(|| ApiError::refused(ErrorCode::ProductUnavailable, "the product doesn't take applications"))?;
    if request.source == ApplicationSource::Web {
        check_human(state, request.turnstile_token.as_deref(), ip).await?;
    }
    let bytes = random_bytes::<32>().map_err(ApiError::internal)?;
    let secret_hash = blake3::hash(&bytes);
    let id = Uuid::now_v7();
    let new = NewApplication {
        id,
        product_id: product.id,
        name: &name,
        email: &email,
        message: message.as_deref(),
        source: request.source.as_str(),
        secret_hash: secret_hash.as_bytes(),
        at: now,
    };
    let mut transaction = state.pool.begin().await?;
    repository::insert(&mut *transaction, &new).await.map_err(|error| {
        if violates_unique(&error, ONE_PENDING_CONSTRAINT) {
            ApiError::refused(ErrorCode::ApplicationPending, "this address already has an application waiting")
        } else {
            error.into()
        }
    })?;
    let metadata = json!({ "product": product.id, "source": request.source.as_str() });
    audit::record(
        &mut *transaction,
        AuditEntry::by_application("application.created", ENTITY, id, ip, now).with_metadata(metadata),
    )
    .await?;
    transaction.commit().await?;
    Ok(Sent { id, secret: encoding::encode(&bytes) })
}

/// The web form's Turnstile answer, when the server has Turnstile keys.
async fn check_human(state: &AppState, answer: Option<&str>, ip: IpAddr) -> Result<(), ApiError> {
    let Some(checker) = state.human_check.as_deref() else { return Ok(()) };
    let failed = || ApiError::refused(ErrorCode::HumanCheckFailed, "the human check failed");
    let answer = answer.filter(|answer| !answer.is_empty() && answer.len() <= MAX_ANSWER).ok_or_else(failed)?;
    match checker.verify(answer, ip).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(failed()),
        Err(error) => Err(ApiError::internal(format!("the human check couldn't run: {error}"))),
    }
}

/// How the application with this secret went.
pub struct Status {
    pub application: ApplicationRow,
    pub product_code: String,
    pub status: ApplicationStatus,
    /// The licence key, formatted, when approved and the licence still has one.
    pub key: Option<String>,
    pub licence_expires_at: Option<DateTime<Utc>>,
}

pub async fn status(state: &AppState, secret: &str, ip: IpAddr, now: DateTime<Utc>) -> Result<Status, ApiError> {
    let unknown = || ApiError::refused(ErrorCode::ApplicationUnknown, "no application has this secret");
    let bytes = encoding::decode_array::<32>(secret.trim()).ok_or_else(unknown)?;
    let application =
        repository::find_by_secret(&state.pool, blake3::hash(&bytes).as_bytes()).await?.ok_or_else(unknown)?;
    let product_code = products::repository::find(&state.pool, application.product_id)
        .await?
        .map(|product| product.code)
        .ok_or_else(unknown)?;
    let (status, key, licence_expires_at) = match (application.state(), application.license_id) {
        (State::Pending, _) => (ApplicationStatus::Pending, None, None),
        (State::Rejected, _) => (ApplicationStatus::Rejected, None, None),
        (State::Approved, None) => (ApplicationStatus::Withdrawn, None, None),
        (State::Approved, Some(licence)) => match key_of(state, licence).await? {
            Some((key, expires_at)) => {
                collected(state, &application, licence, ip, now).await?;
                (ApplicationStatus::Approved, Some(key), expires_at)
            }
            None => (ApplicationStatus::Withdrawn, None, None),
        },
    };
    Ok(Status { application, product_code, status, key, licence_expires_at })
}

/// The licence's key and expiry, unless it was revoked (its key is gone).
async fn key_of(state: &AppState, licence: Uuid) -> Result<Option<(String, Option<DateTime<Utc>>)>, ApiError> {
    let Some(row) = licences::repository::find_many(&state.pool, &[licence]).await?.pop() else { return Ok(None) };
    let Some(stored) = licences::repository::stored_keys(&state.pool, &[licence]).await?.pop() else { return Ok(None) };
    let key = secrets::open(&state.keys, &stored.ciphertext, &stored.nonce, stored.license_id, stored.product_id)
        .map_err(ApiError::internal)?;
    Ok(Some((key.formatted(), row.expires_at)))
}

/// Records the first time the applicant got the key.
async fn collected(
    state: &AppState,
    application: &ApplicationRow,
    licence: Uuid,
    ip: IpAddr,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    if application.key_collected_at.is_some() {
        return Ok(());
    }
    let mut transaction = state.pool.begin().await?;
    if repository::mark_collected(&mut *transaction, application.id, now).await? {
        let entry = AuditEntry::by_application("application.key_collected", ENTITY, application.id, ip, now)
            .with_metadata(json!({ "licence": licence }));
        audit::record(&mut *transaction, entry).await?;
        tracing::info!(application = %application.id, %ip, "an approved application's key was collected");
    }
    transaction.commit().await?;
    Ok(())
}
