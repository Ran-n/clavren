// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/09 09:57:45.588285
//+ Revised: 	2026/11/09 23:01:00.855967
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
pub mod version;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, TimeDelta, Utc};
use serde::Serialize;
use serde_json::json;

use crate::admin_auth::guard::Admin;
use crate::audit::{self, AuditEntry};
use crate::config::UpdateSettings;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::extract::ApiJson;
use files::{FileError, InstallRequest, LatestCheck, ReleaseUrl, UpdateStage, UpdateState, UpdateStatus};
use version::Version;

/// `clavren-update.service` gives up after 15 minutes; an update still "running" well after that
/// died without saying so (power cut, killed), and must not block the button for ever.
const STALE_UPDATE: TimeDelta = TimeDelta::minutes(20);
/// `clavren-update.path` hands a request to the updater within seconds; one still waiting after
/// this was never picked up (the path unit isn't running) and must not block the button for ever.
const UNANSWERED_REQUEST: std::time::Duration = std::time::Duration::from_secs(120);
const UNANSWERED_MESSAGE: &str = "The updater never picked up the request; turn it on with \
    sudo systemctl enable --now clavren-update.path on the server, then try again.";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/updates", get(show))
        .route("/updates/check", post(check_now))
        .route("/updates/install", post(install))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Checking {
    On,
    Off,
    NeverChecked,
}

#[derive(Debug, Serialize)]
pub struct LastCheck {
    pub checked_at: DateTime<Utc>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Available {
    pub version: Version,
    pub release_url: Option<ReleaseUrl>,
    pub published_at: Option<DateTime<Utc>>,
    /// Both bundle files are uploaded; until then the release can't be installed.
    pub installable: bool,
}

#[derive(Debug, Serialize)]
pub struct Updates {
    pub running_version: Version,
    pub checking: Checking,
    pub last_check: Option<LastCheck>,
    /// Set only when the release is newer than the running version.
    pub available: Option<Available>,
    pub backup_drive_mounted: bool,
    /// The current or last update, while it is still relevant (it was for a newer version).
    pub update: Option<UpdateStatus>,
    /// The root updater is installed (a release installation); without it there is no Update button.
    pub updater_installed: bool,
    pub can_install: bool,
}

/// Everything the scripts left, read once per request.
struct Found {
    latest: Option<LatestCheck>,
    status: Option<UpdateStatus>,
    requested: Option<Requested>,
    installer_present: bool,
}

/// An install request still waiting for the updater, and how long it has waited (by the file's
/// age on the system clock, which the updater shares, not the server's own clock).
struct Requested {
    request: InstallRequest,
    waiting: std::time::Duration,
}

/// Logs the unreadable file in full and tells the dashboard what to run.
fn unreadable(error: FileError) -> ApiError {
    tracing::error!(%error, "an update file can't be read");
    ApiError::update_file_unreadable(
        "The update check left a file Clavren can't read; run sudo systemctl start clavren-update-check",
    )
}

fn write_failed(error: FileError) -> ApiError {
    tracing::error!(%error, "an update request can't be written");
    ApiError::internal(error)
}

async fn find(settings: &UpdateSettings) -> Result<Found, ApiError> {
    let (latest_path, request_path) = (settings.dir.join(files::LATEST), settings.dir.join(files::INSTALL_REQUEST));
    let (latest, status, requested, installer) = tokio::join!(
        files::read::<LatestCheck>(&latest_path),
        files::read::<UpdateStatus>(&settings.status),
        files::read::<InstallRequest>(&request_path),
        tokio::fs::try_exists(&settings.installer),
    );
    let installer_present =
        installer.map_err(|source| unreadable(FileError::Read { path: settings.installer.clone(), source }))?;
    let requested = match requested.map_err(unreadable)? {
        Some(request) => {
            files::age(&request_path).await.map_err(unreadable)?.map(|waiting| Requested { request, waiting })
        }
        None => None,
    };
    Ok(Found { latest: latest.map_err(unreadable)?, status: status.map_err(unreadable)?, requested, installer_present })
}

/// The update as the dashboard should see it: a request not yet picked up counts as starting (or
/// as failed once it has waited too long), and a run that died without a word as failed.
fn current_update(found: &Found, running: Version, now: DateTime<Utc>) -> Option<UpdateStatus> {
    if let Some(Requested { request, waiting }) = &found.requested {
        let picked_up = found
            .status
            .as_ref()
            .is_some_and(|status| status.state == UpdateState::Running && status.version == Some(request.version));
        if !picked_up {
            let unanswered = *waiting > UNANSWERED_REQUEST;
            return Some(UpdateStatus {
                version: Some(request.version),
                state: if unanswered { UpdateState::Failed } else { UpdateState::Running },
                stage: Some(UpdateStage::Starting),
                message: unanswered.then(|| UNANSWERED_MESSAGE.to_string()),
                started_at: now,
                finished_at: unanswered.then_some(now),
            });
        }
    }
    let mut status = found.status.clone()?;
    if status.state == UpdateState::Running && now - status.started_at > STALE_UPDATE {
        status.state = UpdateState::Failed;
        status.message =
            Some("The update stopped without finishing; see journalctl -u clavren-update on the server.".into());
    }
    // A finished update is news only until the server runs a newer version; a running one always.
    let relevant = status.state == UpdateState::Running || status.version.is_none_or(|version| version > running);
    relevant.then_some(status)
}

fn view(found: &Found, settings: &UpdateSettings, now: DateTime<Utc>) -> Updates {
    let running = Version::running();
    let checking = match (&found.latest, settings.checking) {
        (_, false) => Checking::Off,
        (None, true) => Checking::NeverChecked,
        (Some(_), true) => Checking::On,
    };
    let latest = found.latest.as_ref().filter(|_| settings.checking);
    let available = latest.and_then(|latest| {
        let version = latest.version.filter(|version| *version > running)?;
        Some(Available {
            version,
            release_url: latest.release_url.clone(),
            published_at: latest.published_at,
            installable: latest.installable,
        })
    });
    let update = current_update(found, running, now);
    let busy = update.as_ref().is_some_and(|update| update.state == UpdateState::Running);
    let can_install = found.installer_present && available.as_ref().is_some_and(|release| release.installable) && !busy;
    Updates {
        running_version: running,
        checking,
        last_check: latest.map(|latest| LastCheck { checked_at: latest.checked_at, error: latest.error.clone() }),
        available,
        backup_drive_mounted: latest.is_some_and(|latest| latest.backup_drive_mounted),
        update,
        updater_installed: found.installer_present,
        can_install,
    }
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

/// Held while a request for the scripts is checked and written (Clavren runs as one process): two
/// at once would share the temporary file and refuse each other.
static REQUESTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn install(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn latest(version: &str) -> LatestCheck {
        LatestCheck {
            checked_at: DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
            version: Some(version.parse().unwrap()),
            release_url: Some(ReleaseUrl::try_from("https://example.com/r".to_string()).unwrap()),
            published_at: None,
            installable: true,
            backup_drive_mounted: true,
            error: None,
        }
    }

    fn found(latest: Option<LatestCheck>) -> Found {
        Found { latest, status: None, requested: None, installer_present: true }
    }

    fn requested(waiting: std::time::Duration) -> Requested {
        Requested { request: InstallRequest { version: newer().parse().unwrap(), skip_backup: false }, waiting }
    }

    fn newer() -> String {
        let Version(major, minor, patch) = Version::running();
        Version(major, minor, patch + 1).to_string()
    }

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_100, 0).unwrap()
    }

    #[test]
    fn only_a_newer_release_is_available() {
        let settings = UpdateSettings::default();
        let running = Version::running().to_string();
        assert!(view(&found(Some(latest(&running))), &settings, now()).available.is_none(), "same version");
        assert!(view(&found(Some(latest("0.0.1"))), &settings, now()).available.is_none(), "older");
        let updates = view(&found(Some(latest(&newer()))), &settings, now());
        assert_eq!(updates.available.unwrap().version.to_string(), newer());
        assert!(updates.can_install);
        assert_eq!(updates.checking, Checking::On);
    }

    #[test]
    fn with_checking_off_nothing_is_offered() {
        let settings = UpdateSettings { checking: false, ..UpdateSettings::default() };
        let updates = view(&found(Some(latest(&newer()))), &settings, now());
        assert_eq!(updates.checking, Checking::Off);
        assert!(updates.available.is_none() && updates.last_check.is_none() && !updates.can_install);
    }

    #[test]
    fn a_request_not_picked_up_yet_counts_as_running() {
        let mut found = found(Some(latest(&newer())));
        found.requested = Some(requested(std::time::Duration::from_secs(5)));
        let updates = view(&found, &UpdateSettings::default(), now());
        let update = updates.update.unwrap();
        assert_eq!((update.state, update.stage), (UpdateState::Running, Some(UpdateStage::Starting)));
        assert!(!updates.can_install);
    }

    #[test]
    fn a_request_being_handled_shows_the_updater_s_progress() {
        let mut found = found(Some(latest(&newer())));
        found.requested = Some(requested(UNANSWERED_REQUEST * 2));
        found.status = Some(UpdateStatus {
            version: Some(newer().parse().unwrap()),
            state: UpdateState::Running,
            stage: Some(UpdateStage::Downloading),
            message: None,
            started_at: now(),
            finished_at: None,
        });
        let update = view(&found, &UpdateSettings::default(), now()).update.unwrap();
        assert_eq!((update.state, update.stage), (UpdateState::Running, Some(UpdateStage::Downloading)));
    }

    /// Regression: with clavren-update.path stopped, a request stayed "starting" for ever and the
    /// Update button never came back.
    #[test]
    fn a_request_nobody_picks_up_fails_and_frees_the_button() {
        let mut found = found(Some(latest(&newer())));
        found.requested = Some(requested(UNANSWERED_REQUEST + std::time::Duration::from_secs(1)));
        let updates = view(&found, &UpdateSettings::default(), now());
        let update = updates.update.unwrap();
        assert_eq!(update.state, UpdateState::Failed);
        assert_eq!(update.message.as_deref(), Some(UNANSWERED_MESSAGE));
        assert!(updates.can_install);
    }

    #[test]
    fn a_run_that_died_counts_as_failed() {
        let mut found = found(Some(latest(&newer())));
        found.status = Some(UpdateStatus {
            version: Some(newer().parse().unwrap()),
            state: UpdateState::Running,
            stage: Some(UpdateStage::Downloading),
            message: None,
            started_at: now() - TimeDelta::hours(1),
            finished_at: None,
        });
        let updates = view(&found, &UpdateSettings::default(), now());
        assert_eq!(updates.update.unwrap().state, UpdateState::Failed);
        assert!(updates.can_install);
    }

    #[test]
    fn a_finished_update_to_this_version_is_old_news() {
        let mut found = found(None);
        found.status = Some(UpdateStatus {
            version: Some(Version::running()),
            state: UpdateState::Succeeded,
            stage: Some(UpdateStage::Restarting),
            message: None,
            started_at: now(),
            finished_at: Some(now()),
        });
        assert!(view(&found, &UpdateSettings::default(), now()).update.is_none());
    }
}
