// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:38:23.315106
//+ Revised: 	2026/11/14 11:39:41.149125
// -----------------------------------------------------------------------------

//! `POST /updates/install`: asking the root updater to install the newer release, once at a time.

use super::files::{self, FileError, InstallRequest, UpdateState};
use super::status::{find, view};
use crate::admin_auth::guard::Admin;
use crate::audit::{self, AuditEntry};
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::extract::ApiJson;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::json;

pub(super) fn write_failed(error: FileError) -> ApiError {
    tracing::error!(%error, "an update request can't be written");
    ApiError::internal(error)
}

/// Held while a request for the scripts is checked and written (Clavren runs as one process): two
/// at once would share the temporary file and refuse each other.
pub(super) static REQUESTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) async fn install(
    State(state): State<AppState>,
    admin: Admin,
    ApiJson(body): ApiJson<InstallRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let settings = &state.settings.updates;
    // One request written at a time: a second install waits, then sees the first as running.
    let _one_at_a_time = REQUESTS.lock().await;
    let found = find(settings).await?;
    let updates = view(&found, settings, admin.actor.at);
    if !found.installer_present {
        return Err(ApiError::conflict(
            "updates_unsupported",
            "this installation has no updater (installed from source?); upgrade it by hand",
        ));
    }
    if updates.update.as_ref().is_some_and(|update| update.state == UpdateState::Running) {
        return Err(ApiError::conflict("update_running", "an update is running already"));
    }
    let Some(release) =
        updates.available.as_ref().filter(|release| release.installable && release.version == body.version)
    else {
        return Err(ApiError::conflict(
            "update_not_available",
            format!("{} is not an installable newer release", body.version),
        ));
    };
    if !updates.backup_drive_mounted && !body.skip_backup {
        return Err(ApiError::conflict(
            "backup_required",
            "the backup drive isn't mounted; confirm updating without a backup",
        ));
    }

    let contents = serde_json::to_vec(&body).map_err(ApiError::internal)?;
    let metadata = json!({ "from": updates.running_version, "to": release.version, "skip_backup": body.skip_backup });
    // No request without its audit entry: the request is written whole first (a failure there
    // rolls the entry back), and the updater only sees it once the entry is committed. A crash or
    // a failed rename between the two leaves an entry for a request that never went out (the
    // admin is told it failed), never a request without its entry.
    let mut transaction = state.pool.begin().await?;
    audit::record(
        &mut *transaction,
        AuditEntry::by_without_id(&admin.actor, "server.update_requested", "server").with_metadata(metadata),
    )
    .await?;
    let staged = files::stage(&settings.dir, files::INSTALL_REQUEST, &contents).await.map_err(write_failed)?;
    if let Err(error) = transaction.commit().await {
        if let Err(leftover) = staged.discard().await {
            tracing::error!(%leftover, "the unsent update request can't be removed");
        }
        return Err(error.into());
    }
    if let Err(error) = staged.publish().await {
        if let Err(leftover) = files::discard(&settings.dir, files::INSTALL_REQUEST).await {
            tracing::error!(%leftover, "the unsent update request can't be removed");
        }
        return Err(write_failed(error));
    }
    tracing::info!(
        admin = %admin.username,
        from = %updates.running_version,
        to = %release.version,
        skip_backup = body.skip_backup,
        "update requested"
    );
    Ok((StatusCode::ACCEPTED, Json(json!({ "requested": true }))))
}
