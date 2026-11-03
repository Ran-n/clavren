// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:13.243670
//+ Revised: 	2026/11/03 04:54:13.243670
// -----------------------------------------------------------------------------

//! Backups are made by `clavren-backup` (a systemd timer, see `deploy/`), which records every
//! successful run in `backup_runs`. The server only reports them: the latest one on the Overview,
//! with a warning when it is older than 48 hours.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor};
use uuid::Uuid;

use crate::admin_auth::guard::Admin;
use crate::http::AppState;
use crate::http::error::ApiError;

/// How many runs the backup script keeps, and the list shows.
pub const KEPT: i64 = 14;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct BackupRun {
    pub id: Uuid,
    pub finished_at: DateTime<Utc>,
    pub archive_name: String,
    pub size_bytes: i64,
}

pub async fn latest(executor: impl PgExecutor<'_>, count: i64) -> Result<Vec<BackupRun>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, finished_at, archive_name, size_bytes FROM backup_runs ORDER BY finished_at DESC LIMIT $1",
    )
    .bind(count)
    .fetch_all(executor)
    .await
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/backups", get(list))
}

async fn list(State(state): State<AppState>, _admin: Admin) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({ "data": latest(&state.pool, KEPT).await? })))
}
