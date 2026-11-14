// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:31.311806
//+ Revised: 	2026/11/14 11:36:31.311806
// -----------------------------------------------------------------------------

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
