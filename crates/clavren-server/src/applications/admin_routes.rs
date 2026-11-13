// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:48.465515
//+ Revised: 	2026/11/11 16:32:53.178580
// -----------------------------------------------------------------------------

//! `/api/admin/applications`: list and search, read, approve (issuing the licence), reject,
//! delete, and the bulk versions of rejecting and deleting.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use super::State as ApplicationState;
use super::decide::{self, ApproveRequest};
use super::repository::{self, Filter};
use super::views::{ApplicationView, EXPANDABLE, present};
use crate::admin_auth::guard::Admin;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::expand::Expand;
use crate::http::extract::{ApiJson, ApiPath, ApiQuery};
use crate::http::fields::contains_pattern;
use crate::http::pagination::{Cursor, Page, PageParams, fetch_count};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/applications", get(list))
        .route("/applications/{id}", get(read).delete(remove))
        .route("/applications/{id}/approve", post(approve))
        .route("/applications/{id}/reject", post(reject))
        .route("/applications/bulk/reject", post(bulk_reject))
        .route("/applications/bulk/delete", post(bulk_delete))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    state: Option<ApplicationState>,
    product: Option<Uuid>,
    /// Part of the name or the email.
    q: Option<String>,
    limit: Option<u32>,
    after: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    _admin: Admin,
    expand: Expand,
    ApiQuery(params): ApiQuery<ListParams>,
) -> Result<Json<Page<ApplicationView>>, ApiError> {
    expand.check(&EXPANDABLE)?;
    let page = PageParams { limit: params.limit, after: params.after };
    let (limit, cursor) = (page.limit()?, page.cursor()?);
    let filter = Filter {
        state: params.state,
        product: params.product,
        pattern: params.q.as_deref().filter(|q| !q.trim().is_empty()).map(contains_pattern),
    };
    let rows = repository::page(&state.pool, &filter, cursor.as_ref(), fetch_count(limit)).await?;
    let page = Page::from_rows(rows, limit, |row| Cursor { key: None, id: row.id });
    let views = present(&state.pool, page.data, &expand).await?;
    Ok(Json(Page { data: views, next_cursor: page.next_cursor }))
}

async fn read(
    State(state): State<AppState>,
    _admin: Admin,
    expand: Expand,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<ApplicationView>, ApiError> {
    expand.check(&EXPANDABLE)?;
    view(&state, id, &expand).await.map(Json)
}

async fn approve(
    State(state): State<AppState>,
    admin: Admin,
    expand: Expand,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<ApproveRequest>,
) -> Result<Json<ApplicationView>, ApiError> {
    expand.check(&EXPANDABLE)?;
    let licence = decide::approve(&state, &admin.actor, id, request).await.map_err(single)?;
    tracing::info!(application = %id, %licence, admin = %admin.username, "licence application approved");
    view(&state, id, &expand).await.map(Json)
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RejectRequest {
    /// Shown to the applicant.
    reason: Option<String>,
}

async fn reject(
    State(state): State<AppState>,
    admin: Admin,
    expand: Expand,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<RejectRequest>,
) -> Result<Json<ApplicationView>, ApiError> {
    expand.check(&EXPANDABLE)?;
    decide::reject(&state, &admin.actor, &[id], request.reason.as_deref()).await.map_err(single)?;
    tracing::info!(application = %id, admin = %admin.username, "licence application rejected");
    view(&state, id, &expand).await.map(Json)
}

async fn remove(
    State(state): State<AppState>,
    admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    decide::delete(&state, &admin.actor, &[id]).await.map_err(single)?;
    tracing::info!(application = %id, admin = %admin.username, "licence application deleted");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BulkReject {
    ids: Vec<Uuid>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BulkIds {
    ids: Vec<Uuid>,
}

async fn bulk_reject(
    State(state): State<AppState>,
    admin: Admin,
    ApiJson(request): ApiJson<BulkReject>,
) -> Result<StatusCode, ApiError> {
    let ids = unique(request.ids);
    decide::reject(&state, &admin.actor, &ids, request.reason.as_deref()).await?;
    tracing::info!(count = ids.len(), admin = %admin.username, "licence applications rejected");
    Ok(StatusCode::NO_CONTENT)
}

async fn bulk_delete(
    State(state): State<AppState>,
    admin: Admin,
    ApiJson(request): ApiJson<BulkIds>,
) -> Result<StatusCode, ApiError> {
    let ids = unique(request.ids);
    decide::delete(&state, &admin.actor, &ids).await?;
    tracing::info!(count = ids.len(), admin = %admin.username, "licence applications deleted");
    Ok(StatusCode::NO_CONTENT)
}

async fn view(state: &AppState, id: Uuid, expand: &Expand) -> Result<ApplicationView, ApiError> {
    let rows = repository::find_many(&state.pool, &[id]).await?;
    present(&state.pool, rows, expand).await?.pop().ok_or_else(ApiError::not_found)
}

/// `ids` without repeats, in their first order: each is decided and audited once.
fn unique(ids: Vec<Uuid>) -> Vec<Uuid> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter().filter(|id| seen.insert(*id)).collect()
}

/// A single-item request names its item in the path; the IDs in a bulk error add nothing.
fn single(error: ApiError) -> ApiError {
    if error.code() == "not_found" { ApiError::not_found() } else { error }
}
