// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:38:24.018173
//+ Revised: 	2026/11/14 11:38:24.018173
// -----------------------------------------------------------------------------

//! Editing licences, one or many at once: customer, expiry, notes, revocation reason and price,
//! all or nothing, each change audited.

use super::lifecycle::{ENTITY, check_batch, lock_existing};
use super::{MAX_NOTES, MAX_REASON, repository, writes};
use crate::audit::{self, Actor};
use crate::customers::{self, CustomerChoice};
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::fields;
use crate::http::patch::Patch;
use crate::money::Price;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use uuid::Uuid;

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
    /// What the key was charged; a test licence has none.
    pub price: Patch<Price>,
}

/// What to do with test licences when a price is set: a single edit refuses, a bulk one leaves
/// them unpriced and counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnTestLicence {
    Refuse,
    Skip,
}

/// What an edit changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Edited {
    /// How many licences changed.
    pub updated: usize,
    /// How many test licences were left without the price asked for.
    pub kept_test: usize,
}

/// [`LicenceChanges`] checked: the changes for the database, with the customer still to resolve.
#[derive(Debug)]
struct Parsed {
    customer: Option<Option<CustomerChoice>>,
    changes: writes::Changes,
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
    let changes = writes::Changes {
        customer_id: None,
        expires_at,
        internal_notes: text("internal_notes", changes.internal_notes, MAX_NOTES)?,
        revocation_reason: text("revocation_reason", changes.revocation_reason, MAX_REASON)?,
        price: if changes.price.is_keep() { None } else { Some(changes.price.apply(None)) },
    };
    let asked = [
        customer.is_some(),
        changes.expires_at.is_some(),
        changes.internal_notes.is_some(),
        changes.revocation_reason.is_some(),
        changes.price.is_some(),
    ];
    let fields: Vec<&'static str> =
        CHANGEABLE.into_iter().zip(asked).filter_map(|(field, on)| on.then_some(field)).collect();
    if fields.is_empty() {
        return Err(ApiError::invalid_request(
            "give at least one of customer, expires_at, internal_notes, revocation_reason, price",
        ));
    }
    Ok(Parsed { customer, changes, fields })
}

/// Applies `changes` to every licence in `ids` or to none; returns how many actually changed. A
/// price set on test licences is refused or skipped as `on_test` says.
pub async fn edit(
    state: &AppState,
    actor: &Actor,
    ids: &[Uuid],
    changes: LicenceChanges,
    on_test: OnTestLicence,
) -> Result<Edited, ApiError> {
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
    let pricing = matches!(changes.price, Some(Some(_)));
    let tests = if pricing { repository::test_ids(&mut *transaction, ids).await? } else { Vec::new() };
    if on_test == OnTestLicence::Refuse && !tests.is_empty() {
        return Err(
            ApiError::conflict("test_licence_price", "test licences can't have a price").with_ids(tests.clone())
        );
    }
    let changed = writes::update(&mut *transaction, ids, &changes).await?;
    // one audit entry per licence that changed, naming the fields that did
    let mut by_fields: Vec<(Vec<&str>, Vec<Uuid>)> = Vec::new();
    for (id, customer, expires_at, notes, reason, price) in changed.iter().copied() {
        let names: Vec<&str> = CHANGEABLE
            .iter()
            .zip([customer, expires_at, notes, reason, price])
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
    let priced: Vec<Uuid> = changed.iter().filter(|changed| changed.5).map(|changed| changed.0).collect();
    if !priced.is_empty() {
        tracing::info!(target: "clavren::licences", licences = ?priced, kept_test = tests.len(), "licence prices changed");
    }
    Ok(Edited { updated: changed.len(), kept_test: tests.len() })
}

/// The fields [`writes::update`] reports on, in its order.
const CHANGEABLE: [&str; 5] = ["customer", "expires_at", "internal_notes", "revocation_reason", "price"];

#[cfg(test)]
mod tests;
