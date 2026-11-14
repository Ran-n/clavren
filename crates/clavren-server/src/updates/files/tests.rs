// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:30.334296
//+ Revised: 	2026/11/14 17:13:08.936879
// -----------------------------------------------------------------------------

use super::*;

#[test]
fn release_pages_must_be_https() {
    for url in ["https://github.com/Ran-n/clavren/releases/tag/v0.4.0", "https://codeberg.org/x"] {
        assert!(ReleaseUrl::try_from(url.to_string()).is_ok(), "{url}");
    }
    for url in ["javascript:alert(1)", "http://github.com/x", "https://", "https://a b", ""] {
        assert!(ReleaseUrl::try_from(url.to_string()).is_err(), "{url}");
    }
}

#[test]
fn a_status_reads_as_the_script_writes_it() {
    let text = r#"{"version": "0.4.0", "state": "running", "stage": "backing_up", "message": null,
                   "started_at": "2026-11-09T10:00:00Z", "finished_at": null}"#;
    let status: UpdateStatus = serde_json::from_str(text).unwrap();
    assert_eq!((status.state, status.stage), (UpdateState::Running, Some(UpdateStage::BackingUp)));
    assert_eq!(status.version, Some(Version(0, 4, 0)));
}

#[test]
fn an_install_request_has_exactly_two_fields() {
    let request = InstallRequest { version: Version(0, 4, 0), skip_backup: true };
    assert_eq!(serde_json::to_string(&request).unwrap(), r#"{"version":"0.4.0","skip_backup":true}"#);
    assert!(serde_json::from_str::<InstallRequest>(r#"{"version":"0.4.0","skip_backup":true,"x":1}"#).is_err());
}

#[tokio::test]
async fn files_are_written_whole_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let updates = dir.path().join("updates");
    assert!(read::<UpdateStatus>(&updates.join("status.json")).await.unwrap().is_none(), "no folder yet");
    write(&updates, CHECK_REQUEST, b"").await.unwrap();
    assert!(updates.join(CHECK_REQUEST).exists());
    assert!(!updates.join("check-request.new").exists(), "no temporary file left");
    let written = age(&updates.join(CHECK_REQUEST)).await.unwrap().unwrap();
    assert!(written < std::time::Duration::from_secs(60), "just written: {written:?}");
    std::fs::remove_file(updates.join(CHECK_REQUEST)).unwrap();
    assert!(age(&updates.join(CHECK_REQUEST)).await.unwrap().is_none(), "gone");
}

#[tokio::test]
async fn a_staged_file_shows_only_once_published() {
    let dir = tempfile::tempdir().unwrap();
    let staged = stage(dir.path(), INSTALL_REQUEST, b"{}").await.unwrap();
    assert!(!dir.path().join(INSTALL_REQUEST).exists(), "not before it is published");
    staged.publish().await.unwrap();
    assert_eq!(std::fs::read(dir.path().join(INSTALL_REQUEST)).unwrap(), b"{}");

    let discarded = stage(dir.path(), INSTALL_REQUEST, b"newer").await.unwrap();
    discarded.discard().await.unwrap();
    assert_eq!(std::fs::read(dir.path().join(INSTALL_REQUEST)).unwrap(), b"{}", "the published one stays");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no temporary file left");
}

#[tokio::test]
async fn something_that_cannot_be_read_is_an_error_not_a_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    // a folder where the file should be
    let read = read::<serde_json::Value>(dir.path()).await;
    assert!(matches!(read, Err(FileError::Read { .. })), "{read:?}");
}

#[tokio::test]
async fn discarding_removes_what_was_staged_or_says_it_cannot() {
    let dir = tempfile::tempdir().unwrap();
    let staged = dir.path().join(format!("{INSTALL_REQUEST}.new"));
    discard(dir.path(), INSTALL_REQUEST).await.unwrap(); // nothing staged: nothing to do
    std::fs::write(&staged, b"{}").unwrap();
    discard(dir.path(), INSTALL_REQUEST).await.unwrap();
    assert!(!staged.exists());
    std::fs::create_dir(&staged).unwrap(); // not a file: it can't be removed as one
    let stuck = discard(dir.path(), INSTALL_REQUEST).await;
    assert!(matches!(stuck, Err(FileError::Write { .. })), "{stuck:?}");
}
