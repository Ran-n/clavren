// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:13.128612
//+ Revised: 	2026/11/03 04:54:13.128612
// -----------------------------------------------------------------------------

//! `GET /api/admin/audit`: who did what, when, from where; newest first.

use std::net::IpAddr;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

use crate::admin_auth::guard::Admin;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::expand::{Expand, Expandable, by_id, distinct};
use crate::http::extract::ApiQuery;
use crate::http::pagination::{Cursor, Page, PageParams, fetch_count};

pub fn routes() -> Router<AppState> {
    Router::new().route("/audit", get(list))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    /// An action (`licence.revoked`) or a prefix ending in a dot (`licence.`).
    action: Option<String>,
    admin: Option<Uuid>,
    entity_type: Option<String>,
    entity: Option<Uuid>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: Option<u32>,
    after: Option<String>,
}

#[derive(Debug, FromRow)]
struct Row {
    id: Uuid,
    admin_user_id: Option<Uuid>,
    action: String,
    entity_type: String,
    entity_id: Option<Uuid>,
    metadata: Value,
    ip_address: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AdminSummary {
    pub id: Uuid,
    pub username: String,
}

#[derive(Debug, Serialize)]
struct Entry {
    id: Uuid,
    admin_user: Option<Expandable<AdminSummary>>,
    action: String,
    entity_type: String,
    entity_id: Option<Uuid>,
    metadata: Value,
    ip_address: Option<IpAddr>,
    created_at: DateTime<Utc>,
}

async fn list(
    State(state): State<AppState>,
    _admin: Admin,
    expand: Expand,
    ApiQuery(params): ApiQuery<ListParams>,
) -> Result<Json<Page<Entry>>, ApiError> {
    expand.check(&["admin_user"])?;
    let page = PageParams { limit: params.limit, after: params.after };
    let (limit, cursor) = (page.limit()?, page.cursor()?);
    let action = params.action.as_deref().map(str::trim).filter(|action| !action.is_empty());
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, admin_user_id, action, entity_type, entity_id, metadata, host(ip_address) AS ip_address, created_at
         FROM audit_logs
         WHERE ($1::text IS NULL OR action = $1 OR (right($1, 1) = '.' AND starts_with(action, $1)))
           AND ($2::uuid IS NULL OR admin_user_id = $2)
           AND ($3::text IS NULL OR entity_type = $3)
           AND ($4::uuid IS NULL OR entity_id = $4)
           AND ($5::timestamptz IS NULL OR created_at >= $5)
           AND ($6::timestamptz IS NULL OR created_at < $6)
           AND ($7::uuid IS NULL OR id < $7)
         ORDER BY id DESC LIMIT $8",
    )
    .bind(action)
    .bind(params.admin)
    .bind(params.entity_type.as_deref())
    .bind(params.entity)
    .bind(params.from)
    .bind(params.to)
    .bind(cursor.map(|cursor| cursor.id))
    .bind(fetch_count(limit))
    .fetch_all(&state.pool)
    .await?;
    let page = Page::from_rows(rows, limit, |row| Cursor { key: None, id: row.id });
    let admins = if expand.has("admin_user") {
        let ids = distinct(&page.data, |row| row.admin_user_id);
        let found: Vec<AdminSummary> = sqlx::query_as("SELECT id, username FROM admin_users WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(&state.pool)
            .await?;
        by_id(found, |admin| admin.id)
    } else {
        Default::default()
    };
    Ok(Json(page.map(|row| Entry {
        id: row.id,
        admin_user: row.admin_user_id.map(|id| Expandable::of(id, &admins)),
        action: row.action,
        entity_type: row.entity_type,
        entity_id: row.entity_id,
        metadata: row.metadata,
        ip_address: row.ip_address.and_then(|ip| ip.parse().ok()),
        created_at: row.created_at,
    })))
}
