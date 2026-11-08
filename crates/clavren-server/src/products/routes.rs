// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.129982
//+ Revised: 	2026/11/07 19:37:00.152489
// -----------------------------------------------------------------------------

//! `/api/admin/products`: list, create, read, edit, and delete products (with their licences), and
//! what deleting one would take with it.

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, response::IntoResponse};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::repository::{self, CODE_CONSTRAINT};
use super::{NewProduct, Product, ProductChanges, Usage, automatic_code};
use crate::admin_auth::guard::Admin;
use crate::audit::{self, AuditEntry};
use crate::database::violates_unique;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::extract::{ApiJson, ApiPath, ApiQuery};
use crate::http::pagination::{Page, PageParams, fetch_count};

const ENTITY: &str = "product";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/products", get(list).post(create))
        .route("/products/{id}", get(read).patch(edit).delete(remove))
        .route("/products/{id}/usage", get(usage))
}

/// The page fields are spelled out: `#[serde(flatten)]` breaks numbers in query strings.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    active: Option<bool>,
    limit: Option<u32>,
    after: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    _admin: Admin,
    ApiQuery(params): ApiQuery<ListParams>,
) -> Result<Json<Page<Product>>, ApiError> {
    let page = PageParams { limit: params.limit, after: params.after };
    let (limit, cursor) = (page.limit()?, page.cursor()?);
    let rows = repository::page(&state.pool, params.active, cursor.as_ref(), fetch_count(limit)).await?;
    Ok(Json(Page::from_rows(rows, limit, repository::cursor_after)))
}

async fn create(
    State(state): State<AppState>,
    admin: Admin,
    ApiJson(request): ApiJson<NewProduct>,
) -> Result<impl IntoResponse, ApiError> {
    request.check_name()?;
    let mut transaction = state.pool.begin().await?;
    let (code, automatic_code) = match request.typed_code() {
        Some(code) => (code.to_string(), false),
        None => (automatic_code::choose(&mut transaction, &request.name).await?, true),
    };
    let product = request.into_product(Uuid::now_v7(), admin.actor.at, &code)?;
    // Two products made with the same code at once: the second gets the conflict; saving again
    // picks the next suffix.
    repository::insert(&mut *transaction, &product).await.map_err(code_taken)?;
    let mut entry = AuditEntry::by(&admin.actor, "product.created", ENTITY, product.id);
    if automatic_code {
        entry = entry.with_metadata(json!({ "automatic_code": true }));
    }
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    tracing::info!(product = %product.code, automatic_code, admin = %admin.username, "product created");
    Ok((StatusCode::CREATED, Json(product)))
}

async fn read(
    State(state): State<AppState>,
    _admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Product>, ApiError> {
    repository::find(&state.pool, id).await?.map(Json).ok_or_else(ApiError::not_found)
}

async fn edit(
    State(state): State<AppState>,
    admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(changes): ApiJson<ProductChanges>,
) -> Result<Json<Product>, ApiError> {
    let mut transaction = state.pool.begin().await?;
    let current = repository::find_for_update(&mut transaction, id).await?.ok_or_else(ApiError::not_found)?;
    let (product, fields) = changes.apply(current.clone(), admin.actor.at)?;
    if fields.is_empty() {
        return Ok(Json(product));
    }
    if product.code != current.code && repository::usage(&mut *transaction, id).await?.licences > 0 {
        return Err(ApiError::conflict(
            "product_code_locked",
            "the code can't change once the product has licences: every activated copy carries it",
        ));
    }
    repository::update(&mut *transaction, &product).await.map_err(code_taken)?;
    let entry = AuditEntry::by(&admin.actor, "product.updated", ENTITY, id).with_metadata(json!({ "fields": fields }));
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    tracing::info!(product = %product.code, admin = %admin.username, ?fields, "product updated");
    Ok(Json(product))
}

async fn usage(
    State(state): State<AppState>,
    _admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Usage>, ApiError> {
    repository::find(&state.pool, id).await?.ok_or_else(ApiError::not_found)?;
    Ok(Json(repository::usage(&state.pool, id).await?))
}

async fn remove(
    State(state): State<AppState>,
    admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    let mut transaction = state.pool.begin().await?;
    let product = repository::find_for_update(&mut transaction, id).await?.ok_or_else(ApiError::not_found)?;
    let deleted = super::delete_with_licences(&mut transaction, &admin.actor, &product).await?;
    transaction.commit().await?;
    tracing::info!(
        product = %product.code,
        licences = deleted.deleted,
        in_use = deleted.in_use,
        admin = %admin.username,
        "product deleted"
    );
    Ok(StatusCode::NO_CONTENT)
}

fn code_taken(error: sqlx::Error) -> ApiError {
    if violates_unique(&error, CODE_CONSTRAINT) { super::code_taken_error() } else { error.into() }
}
