// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.495003
//+ Revised: 	2026/11/10 17:59:21.042715
// -----------------------------------------------------------------------------

//! Changing licences after issuing: editing every field but the product (customer, expiry, notes,
//! and a revoked licence's reason), revoking, resetting (freeing a licence for another computer),
//! and deleting. Each takes a batch of up to 1,000 licences, locks them, and
//! either changes all of them or none, naming the licences that blocked it.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{MAX_BATCH, MAX_NOTES, MAX_REASON, State, repository};
use crate::activations::{self, RevocationReason};
use crate::audit::{self, Actor};
use crate::customers::{self, CustomerChoice};
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::fields;
use crate::http::patch::Patch;

const ENTITY: &str = "licence";

/// The fields of a licence an admin can change; the product, key, state and dates can't be.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LicenceChanges {
    /// An existing customer's ID, a new customer typed in place, or `null` for none.
    pub customer: Patch<CustomerChoice>,
    /// `null` makes the licence permanent; a past date ends it now.
    pub expires_at: Patch<DateTime<Utc>>,
    pub internal_notes: Patch<String>,
    /// Only a revoked licence has one.
    pub revocation_reason: Patch<String>,
}

/// [`LicenceChanges`] checked: the changes for the database, with the customer still to resolve.
#[derive(Debug)]
struct Parsed {
    customer: Option<Option<CustomerChoice>>,
    changes: repository::Changes,
    fields: Vec<&'static str>,
}

fn text(field: &str, patch: Patch<String>, max: usize) -> Result<Option<Option<String>>, ApiError> {
    match patch {
        Patch::Keep => Ok(None),
        Patch::Clear => Ok(Some(None)),
        Patch::Set(text) => Ok(Some(fields::optional(field, Some(&text), max)?)),
    }
}

fn parse(changes: LicenceChanges) -> Result<Parsed, ApiError> {
    let customer = match changes.customer {
        Patch::Keep => None,
        Patch::Clear => Some(None),
        Patch::Set(choice) => Some(Some(choice)),
    };
    let expires_at = if changes.expires_at.is_keep() { None } else { Some(changes.expires_at.apply(None)) };
    let changes = repository::Changes {
        customer_id: None,
        expires_at,
        internal_notes: text("internal_notes", changes.internal_notes, MAX_NOTES)?,
        revocation_reason: text("revocation_reason", changes.revocation_reason, MAX_REASON)?,
    };
    let asked = [
        customer.is_some(),
        changes.expires_at.is_some(),
        changes.internal_notes.is_some(),
        changes.revocation_reason.is_some(),
    ];
    let fields: Vec<&'static str> =
        CHANGEABLE.into_iter().zip(asked).filter_map(|(field, on)| on.then_some(field)).collect();
    if fields.is_empty() {
        return Err(ApiError::invalid_request(
            "give at least one of customer, expires_at, internal_notes, revocation_reason",
        ));
    }
    Ok(Parsed { customer, changes, fields })
}

/// Applies `changes` to every licence in `ids` or to none; returns how many actually changed.
pub async fn edit(state: &AppState, actor: &Actor, ids: &[Uuid], changes: LicenceChanges) -> Result<usize, ApiError> {
    check_batch(ids)?;
    let Parsed { customer, mut changes, fields } = parse(changes)?;
    let mut transaction = state.pool.begin().await?;
    // the customer before the licences, in the order a customer's deletion locks them
    changes.customer_id = match customer {
        None => None,
        Some(None) => Some(None),
        Some(Some(choice)) => Some(Some(customers::choose(&mut transaction, actor, choice).await?.id)),
    };
    let revoked: HashSet<Uuid> = lock_existing(&mut transaction, ids).await?.into_iter().collect();
    let live: Vec<Uuid> = ids.iter().filter(|id| !revoked.contains(id)).copied().collect();
    if changes.revocation_reason.is_some() && !live.is_empty() {
        return Err(
            ApiError::conflict("licence_not_revoked", "only a revoked licence has a revocation reason").with_ids(live)
        );
    }
    let changed = repository::update(&mut *transaction, ids, &changes).await?;
    // one audit entry per licence that changed, naming the fields that did
    let mut by_fields: Vec<(Vec<&str>, Vec<Uuid>)> = Vec::new();
    for (id, customer, expires_at, notes, reason) in changed.iter().copied() {
        let names: Vec<&str> = CHANGEABLE
            .iter()
            .zip([customer, expires_at, notes, reason])
            .filter_map(|(name, on)| on.then_some(*name))
            .collect();
        match by_fields.iter_mut().find(|(group, _)| *group == names) {
            Some((_, group_ids)) => group_ids.push(id),
            None => by_fields.push((names, vec![id])),
        }
    }
    for (names, group_ids) in &by_fields {
        let metadata = json!({ "fields": names });
        audit::record_for_each(&mut *transaction, actor, "licence.updated", ENTITY, group_ids, metadata).await?;
    }
    transaction.commit().await?;
    tracing::info!(asked = ids.len(), changed = changed.len(), ?fields, "licences updated");
    Ok(changed.len())
}

/// The fields [`repository::update`] reports on, in its order.
const CHANGEABLE: [&str; 4] = ["customer", "expires_at", "internal_notes", "revocation_reason"];

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

/// What a delete removed (or, as a dry run, would remove): how many licences, and how many of
/// them were in use on a computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Deleted {
    pub deleted: usize,
    pub in_use: usize,
}

/// Deletes for good, with their activations: their keys read as unknown afterwards, and the
/// computers using them lock at their next check. The audit keeps one entry per licence, with the
/// state it was in. A `dry_run` only counts.
pub async fn delete(state: &AppState, actor: &Actor, ids: &[Uuid], dry_run: bool) -> Result<Deleted, ApiError> {
    check_batch(ids)?;
    let mut transaction = state.pool.begin().await?;
    lock_existing(&mut transaction, ids).await?;
    let deleted = delete_locked(&mut transaction, actor, ids, dry_run).await?;
    if !dry_run {
        transaction.commit().await?;
        tracing::info!(count = deleted.deleted, in_use = deleted.in_use, "licences deleted");
    }
    Ok(deleted)
}

/// Deletes licences already locked in this transaction (unless `dry_run`), auditing each.
pub async fn delete_locked(
    connection: &mut PgConnection,
    actor: &Actor,
    ids: &[Uuid],
    dry_run: bool,
) -> Result<Deleted, ApiError> {
    let states = repository::states(&mut *connection, ids).await?;
    let in_use = states.iter().filter(|(_, state)| state == State::Redeemed.as_str()).count();
    let deleted = Deleted { deleted: states.len(), in_use };
    if dry_run {
        return Ok(deleted);
    }
    repository::delete(connection, ids).await?;
    for was in [State::Unused, State::Redeemed, State::Revoked] {
        let of_state: Vec<Uuid> = states.iter().filter(|(_, state)| state == was.as_str()).map(|(id, _)| *id).collect();
        if !of_state.is_empty() {
            let metadata = json!({ "was": was.as_str() });
            audit::record_for_each(&mut *connection, actor, "licence.deleted", ENTITY, &of_state, metadata).await?;
        }
    }
    Ok(deleted)
}

/// Frees a licence in use so the same key can be activated on another computer; the old computer
/// (every computer, for a test licence) locks at its next check.
pub async fn reset(state: &AppState, actor: &Actor, id: Uuid) -> Result<(), ApiError> {
    reset_unless_moved(state, actor, id, None).await
}

/// Ends one computer's activation, only while it is still one of the licence's computers: an admin
/// ending it never ends another's that took its place meanwhile, nor (on a test licence) the
/// licence's other computers.
pub async fn end_activation(state: &AppState, actor: &Actor, id: Uuid, activation: Uuid) -> Result<(), ApiError> {
    reset_unless_moved(state, actor, id, Some(activation)).await
}

async fn reset_unless_moved(state: &AppState, actor: &Actor, id: Uuid, expected: Option<Uuid>) -> Result<(), ApiError> {
    let mut transaction = state.pool.begin().await?;
    if !lock_existing(&mut transaction, &[id]).await?.is_empty() {
        return Err(ApiError::conflict("licence_revoked", "a revoked licence can't be reset"));
    }
    let metadata = end_computers(&mut transaction, id, expected, actor.at).await?;
    audit::record_for_each(&mut *transaction, actor, "licence.reset", ENTITY, &[id], metadata).await?;
    transaction.commit().await?;
    tracing::info!(licence = %id, "licence reset");
    Ok(())
}

/// Ends the `expected` activation of the licence, or all of them; returns what the audit entry
/// says about it. The licence's lock keeps new activations out, so the answer holds until the commit.
async fn end_computers(
    connection: &mut PgConnection,
    id: Uuid,
    expected: Option<Uuid>,
    at: DateTime<Utc>,
) -> Result<serde_json::Value, ApiError> {
    use activations::repository::{is_active_for, revoke_active, revoke_one};
    let Some(activation) = expected else {
        if revoke_active(&mut *connection, &[id], RevocationReason::Reset, at).await?.is_empty() {
            return Err(ApiError::conflict("licence_not_in_use", "the licence isn't in use on any computer"));
        }
        return Ok(json!({}));
    };
    if !is_active_for(&mut *connection, activation, id).await? {
        return Err(ApiError::conflict("activation_revoked", "this activation has already ended"));
    }
    revoke_one(connection, activation, RevocationReason::Reset, at).await?;
    Ok(json!({ "activation": activation }))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn changes(json: &str) -> LicenceChanges {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn an_empty_change_is_refused() {
        assert_eq!(parse(changes("{}")).unwrap_err().code(), "invalid_request");
    }

    #[test]
    fn removing_the_customer_alone_is_a_change() {
        let parsed = parse(changes(r#"{"customer": null}"#)).unwrap();
        assert!(matches!(parsed.customer, Some(None)));
        assert_eq!(parsed.fields, ["customer"]);
    }

    #[test]
    fn every_field_is_named_and_text_tidied() {
        let parsed = parse(changes(
            r#"{"customer": {"name": "Bea"}, "expires_at": null, "internal_notes": " ", "revocation_reason": " dup "}"#,
        ))
        .unwrap();
        assert_eq!(parsed.fields, ["customer", "expires_at", "internal_notes", "revocation_reason"]);
        assert_eq!(parsed.changes.expires_at, Some(None));
        assert_eq!(parsed.changes.internal_notes, Some(None), "blank notes clear them");
        assert_eq!(parsed.changes.revocation_reason, Some(Some("dup".into())));
    }

    #[test]
    fn an_over_long_reason_is_refused() {
        let body = format!(r#"{{"revocation_reason": "{}"}}"#, "x".repeat(MAX_REASON + 1));
        assert_eq!(parse(changes(&body)).unwrap_err().code(), "invalid_request");
    }

    #[test]
    fn read_only_fields_are_unknown() {
        for field in ["product", "key", "state", "created_at", "revoked_at"] {
            let body = format!(r#"{{"{field}": null}}"#);
            assert!(serde_json::from_str::<LicenceChanges>(&body).is_err(), "{field}");
        }
    }
}
