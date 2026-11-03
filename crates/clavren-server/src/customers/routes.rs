// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:13.529185
//+ Revised: 	2026/11/03 04:54:13.529185
// -----------------------------------------------------------------------------

//! `/api/admin/customers`: search, create, read, edit and delete customers. Logs and audit entries
//! name customers by ID only.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::repository::{self, EMAIL_CONSTRAINT, Order};
use super::{Customer, CustomerChanges, NewCustomer};
use crate::admin_auth::guard::Admin;
use crate::audit::{self, AuditEntry};
use crate::database::violates_unique;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::extract::{ApiJson, ApiPath, ApiQuery};
use crate::http::fields::contains_pattern;
use crate::http::pagination::{Page, PageParams, fetch_count};

const ENTITY: &str = "customer";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/customers", get(list).post(create))
        .route("/customers/{id}", get(read).patch(edit).delete(remove))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    /// Part of the name or email, ignoring case.
    q: Option<String>,
    #[serde(default)]
    order: Order,
    limit: Option<u32>,
    after: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    _admin: Admin,
    ApiQuery(params): ApiQuery<ListParams>,
) -> Result<Json<Page<Customer>>, ApiError> {
    let page = PageParams { limit: params.limit, after: params.after };
    let (limit, cursor) = (page.limit()?, page.cursor()?);
    if cursor.as_ref().is_some_and(|cursor| !repository::cursor_fits(params.order, cursor)) {
        return Err(ApiError::invalid_request("the cursor belongs to a list in another order"));
    }
    let pattern = params.q.as_deref().filter(|q| !q.trim().is_empty()).map(contains_pattern);
    let rows =
        repository::page(&state.pool, pattern.as_deref(), params.order, cursor.as_ref(), fetch_count(limit)).await?;
    Ok(Json(Page::from_rows(rows, limit, repository::cursor_after(params.order))))
}

async fn create(
    State(state): State<AppState>,
    admin: Admin,
    ApiJson(request): ApiJson<NewCustomer>,
) -> Result<impl IntoResponse, ApiError> {
    let customer = request.into_customer(Uuid::now_v7(), admin.actor.at)?;
    let mut transaction = state.pool.begin().await?;
    repository::insert(&mut *transaction, &customer).await.map_err(email_taken)?;
    audit::record(&mut *transaction, AuditEntry::by(&admin.actor, "customer.created", ENTITY, customer.id)).await?;
    transaction.commit().await?;
    tracing::info!(customer = %customer.id, admin = %admin.username, "customer created");
    Ok((StatusCode::CREATED, Json(customer)))
}

async fn read(
    State(state): State<AppState>,
    _admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Customer>, ApiError> {
    repository::find(&state.pool, id).await?.map(Json).ok_or_else(ApiError::not_found)
}

async fn edit(
    State(state): State<AppState>,
    admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(changes): ApiJson<CustomerChanges>,
) -> Result<Json<Customer>, ApiError> {
    let mut transaction = state.pool.begin().await?;
    let current = repository::find_for_update(&mut transaction, id).await?.ok_or_else(ApiError::not_found)?;
    let (customer, fields) = changes.apply(current, admin.actor.at)?;
    if fields.is_empty() {
        return Ok(Json(customer));
    }
    repository::update(&mut *transaction, &customer).await.map_err(email_taken)?;
    let entry = AuditEntry::by(&admin.actor, "customer.updated", ENTITY, id).with_metadata(json!({ "fields": fields }));
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    tracing::info!(customer = %id, admin = %admin.username, ?fields, "customer updated");
    Ok(Json(customer))
}

async fn remove(
    State(state): State<AppState>,
    admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    let mut transaction = state.pool.begin().await?;
    repository::find_for_update(&mut transaction, id).await?.ok_or_else(ApiError::not_found)?;
    repository::delete(&mut *transaction, id).await?;
    audit::record(&mut *transaction, AuditEntry::by(&admin.actor, "customer.deleted", ENTITY, id)).await?;
    transaction.commit().await?;
    tracing::info!(customer = %id, admin = %admin.username, "customer deleted; their licences keep working");
    Ok(StatusCode::NO_CONTENT)
}

fn email_taken(error: sqlx::Error) -> ApiError {
    if violates_unique(&error, EMAIL_CONSTRAINT) {
        ApiError::conflict("customer_email_taken", "another customer already has this email")
    } else {
        error.into()
    }
}
