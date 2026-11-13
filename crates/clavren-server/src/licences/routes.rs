// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.689100
//+ Revised: 	2026/11/12 23:57:45.043382
// -----------------------------------------------------------------------------

//! `/api/admin/licences`: issue, list and search, read, edit, reveal, copy message, revoke, reset,
//! delete, the bulk versions, and CSV export.

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::disclosure::{self, fill_message};
use super::filter::{Expiry, FilterParams, Order};
use super::issue::{self, IssueRequest};
use super::lifecycle::{self, LicenceChanges, OnTestLicence};
use super::views::{EXPANDABLE, LicenceView, present};
use super::{MAX_BATCH, State as LicenceState, repository};
use crate::admin_auth::guard::Admin;
use crate::customers::{Customer, CustomerChoice};
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::expand::Expand;
use crate::http::extract::{ApiJson, ApiPath, ApiQuery};
use crate::http::pagination::{Page, PageParams, fetch_count};
use crate::http::patch::Patch;

/// An export by filter covers at most this many licences.
const MAX_EXPORT: u32 = 10_000;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/licences", get(list).post(issue_licences))
        .route("/licences/{id}", get(read).patch(edit).delete(remove))
        .route("/licences/{id}/reveal", post(reveal))
        .route("/licences/{id}/message", post(message))
        .route("/licences/{id}/revoke", post(revoke))
        .route("/licences/{id}/reset", post(reset))
        .route("/licences/bulk/edit", post(bulk_edit))
        .route("/licences/bulk/revoke", post(bulk_revoke))
        .route("/licences/bulk/delete", post(bulk_delete))
        .route("/licences/export", post(export))
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
        return Err(ApiError::invalid_request("the cursor belongs to a list in another order"));
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

#[derive(Serialize)]
struct IssuedLicence {
    #[serde(flatten)]
    licence: LicenceView,
    key: String,
}

#[derive(Serialize)]
struct IssueResponse {
    licences: Vec<IssuedLicence>,
    customer: Option<Customer>,
    /// The product's message for the customer, with every key; only when there is a customer.
    message: Option<String>,
}

async fn issue_licences(
    State(state): State<AppState>,
    admin: Admin,
    ApiJson(request): ApiJson<IssueRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let issued = issue::issue(&state, &admin.actor, request).await?;
    let ids: Vec<Uuid> = issued.keys.iter().map(|(id, _)| *id).collect();
    let views = present(&state.pool, repository::find_many(&state.pool, &ids).await?, &Expand::default()).await?;
    let keys: Vec<_> = issued.keys.iter().map(|(_, key)| key.clone()).collect();
    let message = issued.customer.as_ref().map(|customer| fill_message(&issued.product, Some(customer), &keys));
    // in the order issued, the same as the keys in the message
    let mut views: std::collections::HashMap<Uuid, LicenceView> =
        views.into_iter().map(|view| (view.id, view)).collect();
    let licences = issued
        .keys
        .iter()
        .filter_map(|(id, key)| views.remove(id).map(|licence| IssuedLicence { licence, key: key.formatted() }))
        .collect();
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
    lifecycle::edit(&state, &admin.actor, &[id], changes, OnTestLicence::Refuse).await.map_err(single)?;
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
) -> Result<Json<lifecycle::Edited>, ApiError> {
    let (ids, changes) = request.split();
    lifecycle::edit(&state, &admin.actor, &ids, changes, OnTestLicence::Skip).await.map(Json)
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

async fn remove(
    State(state): State<AppState>,
    admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    lifecycle::delete(&state, &admin.actor, &[id], false).await.map_err(single)?;
    Ok(StatusCode::NO_CONTENT)
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

async fn view(state: &AppState, id: Uuid, expand: &Expand) -> Result<LicenceView, ApiError> {
    let rows = repository::find_many(&state.pool, &[id]).await?;
    present(&state.pool, rows, expand).await?.pop().ok_or_else(ApiError::not_found)
}

/// A one-licence request answers "not found" plainly, without the list of IDs a batch gets.
fn single(error: ApiError) -> ApiError {
    if error.code() == "not_found" { ApiError::not_found() } else { error }
}
