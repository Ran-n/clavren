// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.653413
//+ Revised: 	2026/11/10 17:59:19.951500
// -----------------------------------------------------------------------------

//! `/api/admin/activations`: every computer a licence was used on. Revoking an activation ends that
//! computer's: it locks at its next check, and the key is free again (a test licence's other
//! computers go on).

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use super::Activation;
use crate::admin_auth::guard::Admin;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::expand::{Expand, Expandable, by_id, distinct};
use crate::http::extract::{ApiPath, ApiQuery};
use crate::http::fields::contains_pattern;
use crate::http::pagination::{Cursor, Page, PageParams, fetch_count};
use crate::licences::{self, LicenceView};

pub fn routes() -> Router<AppState> {
    Router::new().route("/activations", get(list)).route("/activations/{id}/revoke", post(revoke))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ActivationState {
    Active,
    Revoked,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    licence: Option<Uuid>,
    product: Option<Uuid>,
    state: Option<ActivationState>,
    /// Part of the device name, platform or app version.
    q: Option<String>,
    /// `true`: only computers of test licences; `false`: only the others.
    test: Option<bool>,
    limit: Option<u32>,
    after: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    _admin: Admin,
    expand: Expand,
    ApiQuery(params): ApiQuery<ListParams>,
) -> Result<Json<Page<serde_json::Value>>, ApiError> {
    expand.check(&["licence", "licence.product", "licence.customer", "licence.active_activation"])?;
    let page = PageParams { limit: params.limit, after: params.after };
    let (limit, cursor) = (page.limit()?, page.cursor()?);
    let pattern = params.q.as_deref().filter(|q| !q.trim().is_empty()).map(contains_pattern);
    let active = params.state.map(|wanted| wanted == ActivationState::Active);
    let rows: Vec<Activation> = sqlx::query_as(
        "SELECT a.id, a.license_id, a.device_name, a.platform, a.app_version, a.activated_at, a.last_checked_at,
                a.revoked_at, a.revocation_reason
         FROM activations a JOIN licenses l ON l.id = a.license_id
         WHERE ($1::uuid IS NULL OR a.license_id = $1)
           AND ($2::uuid IS NULL OR l.product_id = $2)
           AND ($3::boolean IS NULL OR (a.revoked_at IS NULL) = $3)
           AND ($4::text IS NULL OR a.device_name ILIKE $4 OR a.platform ILIKE $4 OR a.app_version ILIKE $4)
           AND ($5::uuid IS NULL OR a.id < $5)
           AND ($7::boolean IS NULL OR l.is_test = $7)
         ORDER BY a.id DESC LIMIT $6",
    )
    .bind(params.licence)
    .bind(params.product)
    .bind(active)
    .bind(pattern)
    .bind(cursor.map(|cursor| cursor.id))
    .bind(fetch_count(limit))
    .bind(params.test)
    .fetch_all(&state.pool)
    .await?;
    let page = Page::from_rows(rows, limit, |activation| Cursor { key: None, id: activation.id });
    let licences = if expand.has("licence") {
        let ids = distinct(&page.data, |activation| Some(activation.license_id));
        let rows = licences::repository::find_many(&state.pool, &ids).await?;
        by_id(licences::present(&state.pool, rows, &expand.nested("licence")).await?, |licence| licence.id)
    } else {
        Default::default()
    };
    let page = page.try_map(|activation| {
        let licence: Expandable<LicenceView> = Expandable::of(activation.license_id, &licences);
        let mut view = serde_json::to_value(&activation)?;
        view["licence"] = serde_json::to_value(licence)?;
        Ok::<_, serde_json::Error>(view)
    });
    Ok(Json(page.map_err(ApiError::internal)?))
}

/// Revoking a computer's activation frees its licence: the same as a reset.
async fn revoke(
    State(state): State<AppState>,
    admin: Admin,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Activation>, ApiError> {
    let activation = super::repository::find_many(&state.pool, &[id]).await?.pop().ok_or_else(ApiError::not_found)?;
    if activation.revoked_at.is_some() {
        return Err(ApiError::conflict("activation_revoked", "this activation has already ended"));
    }
    licences::end_activation(&state, &admin.actor, activation.license_id, id).await?;
    super::repository::find_many(&state.pool, &[id]).await?.pop().map(Json).ok_or_else(ApiError::not_found)
}
