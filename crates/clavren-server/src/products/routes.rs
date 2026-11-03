// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.129982
//+ Revised: 	2026/11/03 04:54:16.129982
// -----------------------------------------------------------------------------

//! `/api/admin/products`: list, create, read, edit and delete products.

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, response::IntoResponse};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::repository::{self, CODE_CONSTRAINT};
use super::{NewProduct, Product, ProductChanges};
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
    let product = request.into_product(Uuid::now_v7(), admin.actor.at)?;
    let mut transaction = state.pool.begin().await?;
    repository::insert(&mut *transaction, &product).await.map_err(code_taken)?;
    audit::record(&mut *transaction, AuditEntry::by(&admin.actor, "product.created", ENTITY, product.id)).await?;
    transaction.commit().await?;
    tracing::info!(product = %product.code, admin = %admin.username, "product created");
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
    if product.code != current.code && repository::has_licences(&mut *transaction, id).await? {
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

async fn remove(
    State(state): State<AppState>,
    admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    let mut transaction = state.pool.begin().await?;
    let product = repository::find_for_update(&mut transaction, id).await?.ok_or_else(ApiError::not_found)?;
    if repository::has_licences(&mut *transaction, id).await? {
        return Err(ApiError::conflict(
            "product_has_licences",
            "a product with licences can't be deleted; deactivate it instead",
        ));
    }
    repository::delete(&mut *transaction, id).await?;
    audit::record(&mut *transaction, AuditEntry::by(&admin.actor, "product.deleted", ENTITY, id)).await?;
    transaction.commit().await?;
    tracing::info!(product = %product.code, admin = %admin.username, "product deleted");
    Ok(StatusCode::NO_CONTENT)
}

fn code_taken(error: sqlx::Error) -> ApiError {
    if violates_unique(&error, CODE_CONSTRAINT) {
        ApiError::conflict("product_code_taken", "another product already has this code")
    } else {
        error.into()
    }
}
