// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.849802
//+ Revised: 	2026/11/03 04:54:20.513021
// -----------------------------------------------------------------------------

//! `/health/live` (the process answers) and `/health/ready` (it can reach the database). Neither
//! says anything about versions, configuration or the database beyond up or down.

use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

use super::AppState;

const DATABASE_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

pub fn routes() -> Router<AppState> {
    Router::new().route("/health/live", get(live)).route("/health/ready", get(ready))
}

async fn live() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn ready(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let probe = sqlx::query("SELECT 1").execute(&state.pool);
    match tokio::time::timeout(DATABASE_PROBE_TIMEOUT, probe).await {
        Ok(Ok(_)) => (StatusCode::OK, Json(json!({ "status": "ok" }))),
        Ok(Err(error)) => unavailable(&error.to_string()),
        Err(_) => unavailable("no answer within 2 seconds"),
    }
}

fn unavailable(reason: &str) -> (StatusCode, Json<Value>) {
    tracing::warn!(%reason, "not ready: the database is not reachable");
    (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "status": "unavailable" })))
}
