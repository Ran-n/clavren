// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:38:24.215180
//+ Revised: 	2026/11/14 11:38:24.215180
// -----------------------------------------------------------------------------

//! The `/licences/bulk/…` and `/licences/export` handlers: one change, revocation, deletion or CSV
//! for many licences, chosen by ID or by filter.

use super::disclosure::{self};
use super::editing::{self, LicenceChanges, OnTestLicence};
use super::filter::{FilterParams, Order};
use super::lifecycle;
use super::{MAX_BATCH, repository};
use crate::admin_auth::guard::Admin;
use crate::customers::CustomerChoice;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::extract::ApiJson;
use crate::http::pagination::fetch_count;
use crate::http::patch::Patch;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

/// An export by filter covers at most this many licences.
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/licences/bulk/edit", post(bulk_edit))
        .route("/licences/bulk/revoke", post(bulk_revoke))
        .route("/licences/bulk/delete", post(bulk_delete))
        .route("/licences/export", post(export))
}

const MAX_EXPORT: u32 = 10_000;

/// The IDs, and the same fields as [`LicenceChanges`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BulkEdit {
    ids: Vec<Uuid>,
    #[serde(default)]
    customer: Patch<CustomerChoice>,
    #[serde(default)]
    expires_at: Patch<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    internal_notes: Patch<String>,
    #[serde(default)]
    revocation_reason: Patch<String>,
    #[serde(default)]
    price: Patch<crate::money::Price>,
}

impl BulkEdit {
    fn split(self) -> (Vec<Uuid>, LicenceChanges) {
        let changes = LicenceChanges {
            customer: self.customer,
            expires_at: self.expires_at,
            internal_notes: self.internal_notes,
            revocation_reason: self.revocation_reason,
            price: self.price,
        };
        (self.ids, changes)
    }
}

async fn bulk_edit(
    State(state): State<AppState>,
    admin: Admin,
    ApiJson(request): ApiJson<BulkEdit>,
) -> Result<Json<editing::Edited>, ApiError> {
    let (ids, changes) = request.split();
    editing::edit(&state, &admin.actor, &ids, changes, OnTestLicence::Skip).await.map(Json)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BulkRevoke {
    ids: Vec<Uuid>,
    #[serde(default)]
    reason: Option<String>,
}

async fn bulk_revoke(
    State(state): State<AppState>,
    admin: Admin,
    ApiJson(request): ApiJson<BulkRevoke>,
) -> Result<Json<serde_json::Value>, ApiError> {
    lifecycle::revoke(&state, &admin.actor, &request.ids, request.reason.as_deref()).await?;
    Ok(Json(json!({ "revoked": request.ids.len() })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BulkDelete {
    ids: Vec<Uuid>,
    /// Only count what would go.
    #[serde(default)]
    dry_run: bool,
}

async fn bulk_delete(
    State(state): State<AppState>,
    admin: Admin,
    ApiJson(request): ApiJson<BulkDelete>,
) -> Result<Json<lifecycle::Deleted>, ApiError> {
    lifecycle::delete(&state, &admin.actor, &request.ids, request.dry_run).await.map(Json)
}

/// A selection (`ids`) or everything matching a filter (`filter`), but not both.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportRequest {
    ids: Option<Vec<Uuid>>,
    filter: Option<FilterParams>,
}

async fn export(
    State(state): State<AppState>,
    admin: Admin,
    ApiJson(request): ApiJson<ExportRequest>,
) -> Result<Response, ApiError> {
    let rows = match (request.ids, request.filter) {
        (Some(ids), None) => {
            if ids.is_empty() || ids.len() > MAX_BATCH {
                return Err(ApiError::invalid_request(format!("ids must list 1 to {MAX_BATCH} licences")));
            }
            let rows = repository::find_many(&state.pool, &ids).await?;
            let missing: Vec<Uuid> = ids.iter().filter(|id| !rows.iter().any(|row| row.id == **id)).copied().collect();
            if !missing.is_empty() {
                return Err(ApiError::not_found().with_ids(missing));
            }
            rows
        }
        (None, Some(filter)) => {
            let filter = filter.into_filter(&state.keys, admin.actor.at);
            let rows = repository::page(&state.pool, &filter, Order::Newest, None, fetch_count(MAX_EXPORT)).await?;
            if rows.len() > MAX_EXPORT as usize {
                return Err(ApiError::invalid_request(format!(
                    "the filter matches more than {MAX_EXPORT} licences; narrow it"
                )));
            }
            rows
        }
        _ => return Err(ApiError::invalid_request("give either ids or filter")),
    };
    let csv = disclosure::export(&state, &admin.actor, &rows).await?;
    let filename = format!("attachment; filename=\"clavren-licences-{}.csv\"", admin.actor.at.format("%Y%m%d-%H%M%S"));
    Ok(([(header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()), (header::CONTENT_DISPOSITION, filename)], csv)
        .into_response())
}
