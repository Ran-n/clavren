// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:38:23.579523
//+ Revised: 	2026/11/14 11:39:41.047133
// -----------------------------------------------------------------------------

//! What the dashboard is told about updates: the running version, the last check, the newer
//! release and how an update in progress is going, read from the files the scripts write.

use super::files::{self, FileError, InstallRequest, LatestCheck, ReleaseUrl, UpdateStage, UpdateState, UpdateStatus};
use super::version::Version;
use crate::config::UpdateSettings;
use crate::http::error::ApiError;
use chrono::{DateTime, TimeDelta, Utc};
use serde::Serialize;

/// `clavren-update.service` gives up after 15 minutes; an update still "running" well after that
/// died without saying so (power cut, killed), and must not block the button for ever.
const STALE_UPDATE: TimeDelta = TimeDelta::minutes(20);

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
pub(super) struct Found {
    pub(super) latest: Option<LatestCheck>,
    pub(super) status: Option<UpdateStatus>,
    pub(super) requested: Option<Requested>,
    pub(super) installer_present: bool,
}

/// Logs the unreadable file in full and tells the dashboard what to run.
fn unreadable(error: FileError) -> ApiError {
    tracing::error!(%error, "an update file can't be read");
    ApiError::update_file_unreadable(
        "The update check left a file Clavren can't read; run sudo systemctl start clavren-update-check",
    )
}

pub(super) async fn find(settings: &UpdateSettings) -> Result<Found, ApiError> {
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
pub(super) fn current_update(found: &Found, running: Version, now: DateTime<Utc>) -> Option<UpdateStatus> {
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

pub(super) fn view(found: &Found, settings: &UpdateSettings, now: DateTime<Utc>) -> Updates {
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

/// `clavren-update.path` hands a request to the updater within seconds; one still waiting after
/// this was never picked up (the path unit isn't running) and must not block the button for ever.
const UNANSWERED_REQUEST: std::time::Duration = std::time::Duration::from_secs(120);

const UNANSWERED_MESSAGE: &str = "The updater never picked up the request; turn it on with \
    sudo systemctl enable --now clavren-update.path on the server, then try again.";

/// An install request still waiting for the updater, and how long it has waited (by the file's
/// age on the system clock, which the updater shares, not the server's own clock).
pub(super) struct Requested {
    pub(super) request: InstallRequest,
    pub(super) waiting: std::time::Duration,
}

#[cfg(test)]
mod tests;
