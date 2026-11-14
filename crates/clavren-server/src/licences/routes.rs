// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.689100
//+ Revised: 	2026/11/14 11:42:03.398651
// -----------------------------------------------------------------------------

//! `/api/admin/licences`: issue, list and search, read, edit, reveal, copy message, revoke, reset,
//! delete, the bulk versions, and CSV export.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::disclosure::{self, fill_message};
use super::editing::{self, LicenceChanges, OnTestLicence};
use super::filter::{Expiry, FilterParams, Order};
use super::issue::{self, IssueRequest};
use super::lifecycle;
use super::views::{EXPANDABLE, IssueResponse, IssuedLicence, LicenceView, present};
use super::{State as LicenceState, bulk_routes, repository};
use crate::admin_auth::guard::Admin;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::expand::Expand;
use crate::http::extract::{ApiJson, ApiPath, ApiQuery};
use crate::http::pagination::{Page, PageParams, fetch_count, wrong_order};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/licences", get(list).post(issue_licences))
        .route("/licences/{id}", get(read).patch(edit).delete(remove))
        .route("/licences/{id}/reveal", post(reveal))
        .route("/licences/{id}/message", post(message))
        .route("/licences/{id}/revoke", post(revoke))
        .route("/licences/{id}/reset", post(reset))
        .merge(bulk_routes::routes())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    product: Option<Uuid>,
    customer: Option<Uuid>,
    state: Option<LicenceState>,
    expiry: Option<Expiry>,
    q: Option<String>,
    test: Option<bool>,
    not_product: Option<Uuid>,
    not_customer: Option<Uuid>,
    not_state: Option<LicenceState>,
    not_expiry: Option<Expiry>,
    not_q: Option<String>,
    not_test: Option<bool>,
    #[serde(default)]
    order: Order,
    limit: Option<u32>,
    after: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    admin: Admin,
    expand: Expand,
    ApiQuery(params): ApiQuery<ListParams>,
) -> Result<Json<Page<LicenceView>>, ApiError> {
    expand.check(&EXPANDABLE)?;
    let page = PageParams { limit: params.limit, after: params.after };
    let (limit, cursor) = (page.limit()?, page.cursor()?);
    if cursor.as_ref().is_some_and(|cursor| !repository::cursor_fits(params.order, cursor)) {
        return Err(wrong_order());
    }
    let filter = FilterParams {
        product: params.product,
        customer: params.customer,
        state: params.state,
        expiry: params.expiry,
        q: params.q,
        test: params.test,
        not_product: params.not_product,
        not_customer: params.not_customer,
        not_state: params.not_state,
        not_expiry: params.not_expiry,
        not_q: params.not_q,
        not_test: params.not_test,
    }
    .into_filter(&state.keys, admin.actor.at);
    let rows = repository::page(&state.pool, &filter, params.order, cursor.as_ref(), fetch_count(limit)).await?;
    let page = Page::from_rows(rows, limit, repository::cursor_after(params.order));
    let views = present(&state.pool, page.data, &expand).await?;
    Ok(Json(Page { data: views, next_cursor: page.next_cursor }))
}

async fn issue_licences(
    State(state): State<AppState>,
    admin: Admin,
    ApiJson(request): ApiJson<IssueRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let issued = issue::issue(&state, &admin.actor, request).await?;
    let ids: Vec<Uuid> = issued.keys.iter().map(|(id, _)| *id).collect();
    let views = present(&state.pool, repository::find_many(&state.pool, &ids).await?, &Expand::default()).await?;
    // In the order issued, the same as the keys in the message. A licence another admin deleted
    // since the commit is left out of both: its key is unknown already.
    let mut views: std::collections::HashMap<Uuid, LicenceView> =
        views.into_iter().map(|view| (view.id, view)).collect();
    let (keys, licences): (Vec<_>, Vec<_>) = issued
        .keys
        .iter()
        .filter_map(|(id, key)| Some((key.clone(), IssuedLicence { licence: views.remove(id)?, key: key.formatted() })))
        .unzip();
    let message = issued.customer.as_ref().map(|customer| fill_message(&issued.product, Some(customer), &keys));
    tracing::info!(count = ids.len(), product = %issued.product.code, admin = %admin.username, "licences issued");
    Ok((StatusCode::CREATED, Json(IssueResponse { licences, customer: issued.customer, message })))
}

async fn read(
    State(state): State<AppState>,
    _admin: Admin,
    expand: Expand,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<LicenceView>, ApiError> {
    expand.check(&EXPANDABLE)?;
    view(&state, id, &expand).await.map(Json)
}

async fn edit(
    State(state): State<AppState>,
    admin: Admin,
    expand: Expand,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(changes): ApiJson<LicenceChanges>,
) -> Result<Json<LicenceView>, ApiError> {
    expand.check(&EXPANDABLE)?;
    editing::edit(&state, &admin.actor, &[id], changes, OnTestLicence::Refuse).await.map_err(single)?;
    view(&state, id, &expand).await.map(Json)
}

async fn reveal(
    State(state): State<AppState>,
    admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let key = disclosure::reveal(&state, &admin.actor, id).await?;
    Ok(Json(json!({ "key": key.formatted() })))
}

async fn message(
    State(state): State<AppState>,
    admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let message = disclosure::message(&state, &admin.actor, id).await?;
    Ok(Json(json!({ "message": message })))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RevokeRequest {
    reason: Option<String>,
}

async fn revoke(
    State(state): State<AppState>,
    admin: Admin,
    expand: Expand,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<RevokeRequest>,
) -> Result<Json<LicenceView>, ApiError> {
    expand.check(&EXPANDABLE)?;
    lifecycle::revoke(&state, &admin.actor, &[id], request.reason.as_deref()).await.map_err(single)?;
    view(&state, id, &expand).await.map(Json)
}

async fn reset(
    State(state): State<AppState>,
    admin: Admin,
    expand: Expand,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<LicenceView>, ApiError> {
    expand.check(&EXPANDABLE)?;
    lifecycle::reset(&state, &admin.actor, id).await.map_err(single)?;
    view(&state, id, &expand).await.map(Json)
}

async fn remove(
    State(state): State<AppState>,
    admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    lifecycle::delete(&state, &admin.actor, &[id], false).await.map_err(single)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn view(state: &AppState, id: Uuid, expand: &Expand) -> Result<LicenceView, ApiError> {
    let rows = repository::find_many(&state.pool, &[id]).await?;
    present(&state.pool, rows, expand).await?.pop().ok_or_else(ApiError::not_found)
}

/// A one-licence request answers "not found" plainly, without the list of IDs a batch gets.
fn single(error: ApiError) -> ApiError {
    if error.code() == "not_found" { ApiError::not_found() } else { error }
}
