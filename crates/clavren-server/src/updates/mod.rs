// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/09 09:57:45.588285
//+ Revised: 	2026/11/14 11:38:23.698485
// -----------------------------------------------------------------------------

//! Server updates: which version runs, whether a newer release is out, and installing it.
//!
//! The server makes no outgoing connection and never runs anything as root. Two scripts outside
//! it do the work (`deploy/clavren-update-check.sh` as the clavren user, `deploy/clavren-update.sh`
//! as root), each started by a systemd path unit when the server writes its request file; see
//! [`files`] and `docs/design-decisions.md`.
//!
//! - `GET /updates`: the running version, the last check, the newer release, the update status.
//! - `POST /updates/check`: check now.
//! - `POST /updates/install`: install the newer release (backing up first, unless told not to).

pub mod files;
mod install;
mod status;
pub mod version;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::admin_auth::guard::Admin;
use crate::http::AppState;
use crate::http::error::ApiError;
use install::{REQUESTS, install, write_failed};
use status::{find, view};

pub use status::{Available, Checking, LastCheck, Updates};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/updates", get(show))
        .route("/updates/check", post(check_now))
        .route("/updates/install", post(install))
}

async fn show(State(state): State<AppState>, admin: Admin) -> Result<Json<Updates>, ApiError> {
    let settings = &state.settings.updates;
    let found = find(settings).await?;
    Ok(Json(view(&found, settings, admin.actor.at)))
}

async fn check_now(
    State(state): State<AppState>,
    admin: Admin,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let settings = &state.settings.updates;
    if !settings.checking {
        tracing::warn!(admin = %admin.username, "update check refused: CLAVREN_UPDATE_SOURCE is off");
        return Err(ApiError::conflict("updates_off", "checking for updates is off (CLAVREN_UPDATE_SOURCE=off)"));
    }
    let _one_at_a_time = REQUESTS.lock().await;
    files::write(&settings.dir, files::CHECK_REQUEST, b"").await.map_err(write_failed)?;
    tracing::info!(admin = %admin.username, "update check requested");
    Ok((StatusCode::ACCEPTED, Json(json!({ "requested": true }))))
}
