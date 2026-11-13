// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:48.540443
//+ Revised: 	2026/11/12 23:57:44.108820
// -----------------------------------------------------------------------------

//! The administrator's decisions. Approving issues the application's licence (one, for its
//! product, with the customer, expiry and notes the administrator chose) and records the approval
//! in the same transaction. Rejecting records an optional reason the applicant is shown. Either
//! happens once: a decided application can't be decided again, only deleted. Bulk requests are
//! all-or-nothing, with an error naming the applications that blocked them.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{ApplicationRow, MAX_REASON, State, repository};
use crate::audit::{self, Actor, AuditEntry};
use crate::customers::CustomerChoice;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::fields;
use crate::http::patch::Patch;
use crate::licences::{self, IssueRequest, MAX_BATCH};
use crate::money::Price;
use crate::products;

const ENTITY: &str = "application";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ApproveRequest {
    /// Who the licence is for: usually a new customer from the applicant's name and email, or the
    /// existing customer with that email. None: no customer.
    pub customer: Option<CustomerChoice>,
    pub expires_at: Option<DateTime<Utc>>,
    pub internal_notes: Option<String>,
    /// What the key is charged; none unless given (an application is usually not a sale).
    pub price: Option<Price>,
}

/// Approves `id`: its licence is issued and linked. Returns the licence's ID.
pub async fn approve(state: &AppState, actor: &Actor, id: Uuid, request: ApproveRequest) -> Result<Uuid, ApiError> {
    let mut transaction = state.pool.begin().await?;
    // the product first, as issuing and deleting a product lock it: then the application
    let product = repository::product_of(&mut *transaction, id).await?.ok_or_else(ApiError::not_found)?;
    products::repository::find_for_share(&mut transaction, product).await?.ok_or_else(ApiError::not_found)?;
    let application = repository::lock(&mut transaction, &[id]).await?.pop().ok_or_else(ApiError::not_found)?;
    undecided(&[application])?;
    let request = IssueRequest {
        product,
        customer: request.customer,
        quantity: 1,
        expires_at: request.expires_at,
        internal_notes: request.internal_notes,
        test: false,
        price: request.price.map_or(Patch::Clear, Patch::Set),
    };
    let issued = licences::issue_in(state, &mut transaction, actor, request).await?;
    let licence =
        issued.keys.first().map(|(licence, _)| *licence).ok_or_else(|| ApiError::internal("no licence issued"))?;
    repository::approve(&mut *transaction, id, licence, actor.at).await?;
    let entry = AuditEntry::by(actor, "application.approved", ENTITY, id).with_metadata(json!({ "licence": licence }));
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    Ok(licence)
}

/// Rejects every application in `ids`, with the same optional reason.
pub async fn reject(state: &AppState, actor: &Actor, ids: &[Uuid], reason: Option<&str>) -> Result<(), ApiError> {
    let reason = fields::optional("reason", reason, MAX_REASON)?;
    let mut transaction = state.pool.begin().await?;
    let found = locked(&mut transaction, ids).await?;
    undecided(&found)?;
    repository::reject(&mut *transaction, ids, reason.as_deref(), actor.at).await?;
    let metadata = json!({ "with_reason": reason.is_some() });
    audit::record_for_each(&mut *transaction, actor, "application.rejected", ENTITY, ids, metadata).await?;
    transaction.commit().await?;
    Ok(())
}

/// Deletes every application in `ids`, in any state. An approved one's licence stays.
pub async fn delete(state: &AppState, actor: &Actor, ids: &[Uuid]) -> Result<(), ApiError> {
    let mut transaction = state.pool.begin().await?;
    locked(&mut transaction, ids).await?;
    repository::delete(&mut *transaction, ids).await?;
    audit::record_for_each(&mut *transaction, actor, "application.deleted", ENTITY, ids, json!({})).await?;
    transaction.commit().await?;
    Ok(())
}

/// The applications of `ids`, locked; 404 naming those that don't exist.
async fn locked(connection: &mut sqlx::PgConnection, ids: &[Uuid]) -> Result<Vec<ApplicationRow>, ApiError> {
    if ids.is_empty() || ids.len() > MAX_BATCH {
        return Err(ApiError::invalid_request(format!("ids must hold between 1 and {MAX_BATCH} applications")));
    }
    let found = repository::lock(connection, ids).await?;
    let missing: Vec<Uuid> = ids.iter().filter(|id| !found.iter().any(|row| row.id == **id)).copied().collect();
    if !missing.is_empty() {
        return Err(ApiError::not_found().with_ids(missing));
    }
    Ok(found)
}

/// 409 naming the applications that were already decided.
fn undecided(rows: &[ApplicationRow]) -> Result<(), ApiError> {
    let decided: Vec<Uuid> = rows.iter().filter(|row| row.state() != State::Pending).map(|row| row.id).collect();
    if decided.is_empty() {
        return Ok(());
    }
    Err(ApiError::conflict("application_decided", "the application was already approved or rejected").with_ids(decided))
}
