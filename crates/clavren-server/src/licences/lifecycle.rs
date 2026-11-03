// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.495003
//+ Revised: 	2026/11/03 04:54:15.495003
// -----------------------------------------------------------------------------

//! Changing licences after issuing: editing expiry and notes, revoking, and resetting (freeing a
//! licence for another computer). Each takes a batch of up to 1,000 licences, locks them, and
//! either changes all of them or none, naming the licences that blocked it.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{MAX_BATCH, MAX_NOTES, MAX_REASON, repository};
use crate::activations::{self, RevocationReason};
use crate::audit::{self, Actor};
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::fields;
use crate::http::patch::Patch;

const ENTITY: &str = "licence";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LicenceChanges {
    /// `null` makes the licence permanent; a past date ends it now.
    pub expires_at: Patch<DateTime<Utc>>,
    pub internal_notes: Patch<String>,
}

pub async fn edit(state: &AppState, actor: &Actor, ids: &[Uuid], changes: LicenceChanges) -> Result<(), ApiError> {
    check_batch(ids)?;
    let notes = match changes.internal_notes {
        Patch::Keep => None,
        Patch::Clear => Some(None),
        Patch::Set(text) => Some(fields::optional("internal_notes", Some(&text), MAX_NOTES)?),
    };
    let expires_at = match changes.expires_at {
        Patch::Keep => None,
        other => Some(other.apply(None)),
    };
    let fields: Vec<&str> = [("expires_at", expires_at.is_some()), ("internal_notes", notes.is_some())]
        .into_iter()
        .filter_map(|(f, on)| on.then_some(f))
        .collect();
    if fields.is_empty() {
        return Err(ApiError::invalid_request("give expires_at and/or internal_notes to change"));
    }
    let mut transaction = state.pool.begin().await?;
    lock_existing(&mut transaction, ids).await?;
    repository::update(&mut *transaction, ids, expires_at, notes.as_ref().map(Option::as_deref)).await?;
    audit::record_for_each(&mut *transaction, actor, "licence.updated", ENTITY, ids, json!({ "fields": fields }))
        .await?;
    transaction.commit().await?;
    tracing::info!(count = ids.len(), ?fields, "licences updated");
    Ok(())
}

/// Revokes for good: the keys are deleted, and the computers using them lock at their next check.
pub async fn revoke(state: &AppState, actor: &Actor, ids: &[Uuid], reason: Option<&str>) -> Result<(), ApiError> {
    check_batch(ids)?;
    let reason = fields::optional("reason", reason, MAX_REASON)?;
    let mut transaction = state.pool.begin().await?;
    let revoked = lock_existing(&mut transaction, ids).await?;
    if !revoked.is_empty() {
        return Err(ApiError::conflict("licence_revoked", "some licences are already revoked").with_ids(revoked));
    }
    repository::revoke(&mut transaction, ids, reason.as_deref(), actor.at).await?;
    let deactivated =
        activations::repository::revoke_active(&mut *transaction, ids, RevocationReason::LicenceRevoked, actor.at)
            .await?;
    audit::record_for_each(&mut *transaction, actor, "licence.revoked", ENTITY, ids, json!({})).await?;
    transaction.commit().await?;
    tracing::info!(count = ids.len(), computers = deactivated.len(), "licences revoked");
    Ok(())
}

/// Frees a licence in use so the same key can be activated on another computer; the old computer
/// locks at its next check.
pub async fn reset(state: &AppState, actor: &Actor, id: Uuid) -> Result<(), ApiError> {
    let mut transaction = state.pool.begin().await?;
    if !lock_existing(&mut transaction, &[id]).await?.is_empty() {
        return Err(ApiError::conflict("licence_revoked", "a revoked licence can't be reset"));
    }
    let freed =
        activations::repository::revoke_active(&mut *transaction, &[id], RevocationReason::Reset, actor.at).await?;
    if freed.is_empty() {
        return Err(ApiError::conflict("licence_not_in_use", "the licence isn't in use on any computer"));
    }
    audit::record_for_each(&mut *transaction, actor, "licence.reset", ENTITY, &[id], json!({})).await?;
    transaction.commit().await?;
    tracing::info!(licence = %id, "licence reset");
    Ok(())
}

/// Locks the licences; fails naming any that don't exist. Returns the ones already revoked.
async fn lock_existing(connection: &mut PgConnection, ids: &[Uuid]) -> Result<Vec<Uuid>, ApiError> {
    let found = repository::lock(connection, ids).await?;
    let existing: HashSet<Uuid> = found.iter().map(|(id, _)| *id).collect();
    let missing: Vec<Uuid> = ids.iter().filter(|id| !existing.contains(id)).copied().collect();
    if !missing.is_empty() {
        return Err(ApiError::not_found().with_ids(missing));
    }
    Ok(found.into_iter().filter_map(|(id, revoked)| revoked.then_some(id)).collect())
}

fn check_batch(ids: &[Uuid]) -> Result<(), ApiError> {
    if ids.is_empty() || ids.len() > MAX_BATCH {
        return Err(ApiError::invalid_request(format!("ids must list 1 to {MAX_BATCH} licences")));
    }
    let distinct: HashSet<&Uuid> = ids.iter().collect();
    if distinct.len() != ids.len() {
        return Err(ApiError::invalid_request("ids lists a licence more than once"));
    }
    Ok(())
}
