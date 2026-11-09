// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/09 09:57:45.846323
//+ Revised: 	2026/11/09 11:10:33.517266
// -----------------------------------------------------------------------------

//! `/api/admin/updates`: what the update scripts left in the data folder, as the dashboard sees
//! it, and the requests the server writes for them.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use std::path::{Path, PathBuf};

use axum::http::{Method, StatusCode};
use clavren::config::UpdateSettings;
use clavren::updates::version::Version;
use common::api::{Admin, Call};
use common::{capture_logs, test_settings};
use serde_json::{Value, json};
use sqlx::PgPool;
use tempfile::TempDir;

struct Updates {
    admin: Admin,
    dir: PathBuf,
    status: PathBuf,
    _temporary: TempDir,
}

fn running() -> String {
    Version::running().to_string()
}

fn newer() -> String {
    let Version(major, minor, patch) = Version::running();
    Version(major, minor + 1, patch).to_string()
}

fn older() -> &'static str {
    "0.0.1"
}

impl Updates {
    /// A server whose updates folder is a new temporary one, with the updater installed unless
    /// `installer` is false.
    async fn new(pool: PgPool, checking: bool, installer: bool) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let dir = temporary.path().join("updates");
        std::fs::create_dir(&dir).unwrap();
        let script = temporary.path().join("clavren-update.sh");
        if installer {
            std::fs::write(&script, "#!/bin/sh\n").unwrap();
        }
        let status = temporary.path().join("clavren-update").join("status.json");
        std::fs::create_dir(status.parent().unwrap()).unwrap();
        let updates = UpdateSettings { dir: dir.clone(), checking, installer: script, status: status.clone() };
        let settings = clavren::http::Settings { updates, ..test_settings() };
        Self { admin: Admin::with_settings(pool, settings).await, dir, status, _temporary: temporary }
    }

    fn write(&self, name: &str, value: &Value) {
        std::fs::write(self.dir.join(name), value.to_string()).unwrap();
    }

    fn write_status(&self, value: &Value) {
        std::fs::write(&self.status, value.to_string()).unwrap();
    }

    fn latest(&self, version: &str, changes: Value) {
        let mut latest = json!({
            "checked_at": "2027-01-15T14:02:00Z",
            "version": version,
            "release_url": format!("https://github.com/Ran-n/clavren/releases/tag/v{version}"),
            "published_at": "2027-01-15T10:00:00Z",
            "installable": true,
            "bundle_url": "https://github.com/x.tar.gz",
            "checksum_url": "https://github.com/x.tar.gz.b3",
            "backup_drive_mounted": true,
            "error": null,
        });
        for (name, value) in changes.as_object().unwrap() {
            latest[name] = value.clone();
        }
        self.write("latest.json", &latest);
    }

    async fn get(&self) -> Value {
        let reply = self.admin.get("/api/admin/updates").await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
        reply.body
    }

    async fn install(&self, body: Value) -> common::api::Reply {
        self.admin.call(Method::POST, "/api/admin/updates/install", Some(body)).await
    }

    async fn audited(&self) -> Vec<Value> {
        sqlx::query_scalar(
            "SELECT jsonb_build_object('entity_type', entity_type, 'entity_id', entity_id, 'metadata', metadata)
             FROM audit_logs WHERE action = 'server.update_requested'",
        )
        .fetch_all(self.admin.pool())
        .await
        .unwrap()
    }
}

fn exists(dir: &Path, name: &str) -> bool {
    dir.join(name).exists()
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn before_the_first_check_only_the_version_is_known(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    let body = updates.get().await;
    assert_eq!(body["running_version"], running());
    assert_eq!(body["checking"], "never_checked");
    assert_eq!((&body["last_check"], &body["available"], &body["update"]), (&Value::Null, &Value::Null, &Value::Null));
    assert_eq!((&body["can_install"], &body["updater_installed"]), (&json!(false), &json!(true)));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_newer_release_is_available(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({}));
    let body = updates.get().await;
    assert_eq!(body["checking"], "on");
    assert_eq!(body["last_check"], json!({ "checked_at": "2027-01-15T14:02:00Z", "error": null }));
    assert_eq!(
        body["available"],
        json!({
            "version": newer(),
            "release_url": format!("https://github.com/Ran-n/clavren/releases/tag/v{}", newer()),
            "published_at": "2027-01-15T10:00:00Z",
            "installable": true,
        })
    );
    assert_eq!((&body["backup_drive_mounted"], &body["can_install"]), (&json!(true), &json!(true)));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_same_or_an_older_release_is_not_offered(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    for version in [running(), older().to_string()] {
        updates.latest(&version, json!({}));
        let body = updates.get().await;
        assert_eq!(body["available"], Value::Null, "{version}");
        assert_eq!(body["checking"], "on");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_failed_check_is_reported_and_keeps_the_known_release(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({ "error": "could not reach api.github.com" }));
    let body = updates.get().await;
    assert_eq!(body["last_check"]["error"], "could not reach api.github.com");
    assert_eq!(body["available"]["version"], newer());
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_release_without_its_files_is_not_installable_yet(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({ "installable": false }));
    let body = updates.get().await;
    assert_eq!((&body["available"]["installable"], &body["can_install"]), (&json!(false), &json!(false)));
    let reply = updates.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "update_not_available"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_unreadable_file_is_an_error_and_logged(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    std::fs::write(updates.dir.join("latest.json"), "{ not json").unwrap();
    let (logs, _guard) = capture_logs();
    let reply = updates.admin.get("/api/admin/updates").await;
    assert_eq!((reply.status, reply.error()), (StatusCode::INTERNAL_SERVER_ERROR, "update_file_unreadable"));
    let text = logs.text();
    assert!(text.contains("ERROR") && text.contains("latest.json"), "{text}");

    // a release page that isn't https:// can't be linked to, so the file is refused too
    updates.latest(&newer(), json!({ "release_url": "javascript:alert(1)" }));
    let reply = updates.admin.get("/api/admin/updates").await;
    assert_eq!(reply.error(), "update_file_unreadable");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn check_now_writes_a_request_for_the_check(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    let reply = Call::new(Method::POST, "/api/admin/updates/check")
        .signed_in(&updates.admin.browser)
        .csrf("wrong")
        .send(&updates.admin.fixture.app)
        .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert!(!exists(&updates.dir, "check-request"));

    let reply = updates.admin.call(Method::POST, "/api/admin/updates/check", None).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED, "{:?}", reply.body);
    assert!(exists(&updates.dir, "check-request"));
    assert!(!exists(&updates.dir, "check-request.new"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn with_checking_off_nothing_is_checked_or_offered(pool: PgPool) {
    let updates = Updates::new(pool, false, true).await;
    updates.latest(&newer(), json!({}));
    let body = updates.get().await;
    assert_eq!((&body["checking"], &body["available"]), (&json!("off"), &Value::Null));
    let reply = updates.admin.call(Method::POST, "/api/admin/updates/check", None).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "updates_off"));
    assert!(!exists(&updates.dir, "check-request"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn updates_need_a_session(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    for (method, path) in [
        (Method::GET, "/api/admin/updates"),
        (Method::POST, "/api/admin/updates/check"),
        (Method::POST, "/api/admin/updates/install"),
    ] {
        let reply = Call::new(method, path).send(&updates.admin.fixture.app).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn installing_writes_the_request_and_audits_it(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({}));
    let reply = updates.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED, "{:?}", reply.body);

    let written: Value = serde_json::from_slice(&std::fs::read(updates.dir.join("request.json")).unwrap()).unwrap();
    assert_eq!(written, json!({ "version": newer(), "skip_backup": false }));
    assert_eq!(
        updates.audited().await,
        [json!({
            "entity_type": "server",
            "entity_id": null,
            "metadata": { "from": running(), "to": newer(), "skip_backup": false },
        })]
    );

    let body = updates.get().await;
    assert_eq!((&body["update"]["state"], &body["update"]["stage"]), (&json!("running"), &json!("starting")));
    assert_eq!(body["can_install"], false);
    let again = updates.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!((again.status, again.error()), (StatusCode::CONFLICT, "update_running"));

    let csrf = Call::new(Method::POST, "/api/admin/updates/install")
        .signed_in(&updates.admin.browser)
        .csrf("wrong")
        .json(json!({ "version": newer(), "skip_backup": false }))
        .send(&updates.admin.fixture.app)
        .await;
    assert_eq!(csrf.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn installing_is_refused_when_it_cannot_go_ahead(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({ "backup_drive_mounted": false }));
    let refused = |body: Value| async { updates.install(body).await };

    let reply = refused(json!({ "version": running(), "skip_backup": true })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "update_not_available"), "not the newer one");
    let reply = refused(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "backup_required"));
    for body in [
        json!({ "version": format!("v{}", newer()), "skip_backup": true }),
        json!({ "version": "1.0.0; rm -rf /", "skip_backup": true }),
        json!({ "version": newer(), "skip_backup": "yes" }),
        json!({ "version": newer(), "skip_backup": true, "url": "https://evil.example.com" }),
        json!({ "version": newer() }),
    ] {
        let reply = refused(body.clone()).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{body}");
    }
    assert!(!exists(&updates.dir, "request.json"));
    assert!(updates.audited().await.is_empty());

    updates.write_status(&json!({ "version": newer(), "state": "running", "stage": "downloading", "message": null,
                 "started_at": "2030-01-15T09:59:00Z", "finished_at": null }));
    let reply = refused(json!({ "version": newer(), "skip_backup": true })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "update_running"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn without_the_updater_there_is_nothing_to_install_with(pool: PgPool) {
    let unsupported = Updates::new(pool, true, false).await;
    unsupported.latest(&newer(), json!({}));
    let body = unsupported.get().await;
    assert_eq!((&body["can_install"], &body["updater_installed"]), (&json!(false), &json!(false)));
    let reply = unsupported.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "updates_unsupported"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn without_a_backup_drive_an_update_must_say_so(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({ "backup_drive_mounted": false }));
    assert_eq!(updates.get().await["backup_drive_mounted"], false);
    let reply = updates.install(json!({ "version": newer(), "skip_backup": true })).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED, "{:?}", reply.body);
    assert_eq!(updates.audited().await[0]["metadata"]["skip_backup"], true);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_request_that_cannot_be_written_leaves_no_audit_entry(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({}));
    // the temporary file's name is taken by a folder, so writing the request fails
    std::fs::create_dir(updates.dir.join("request.json.new")).unwrap();
    let (logs, _guard) = capture_logs();
    let reply = updates.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(logs.text().contains("request.json"), "{}", logs.text());
    assert!(updates.audited().await.is_empty(), "the audit entry was rolled back");
    assert!(!exists(&updates.dir, "request.json"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_update_status_comes_back(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({}));
    let started = common::test_start().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    for (state, stage, message) in [
        ("running", "backing_up", Value::Null),
        ("failed", "checking", json!("The download didn't match its checksum, so nothing was installed.")),
    ] {
        let status = json!({ "version": newer(), "state": state, "stage": stage, "message": message,
                             "started_at": started, "finished_at": null });
        updates.write_status(&status);
        let body = updates.get().await;
        assert_eq!(body["update"], status, "{state}");
        assert_eq!(body["can_install"], state == "failed");
    }

    // an update to the version that now runs is over: nothing to show
    updates.write_status(&json!({ "version": running(), "state": "succeeded", "stage": "restarting", "message": null,
                 "started_at": started, "finished_at": started }));
    assert_eq!(updates.get().await["update"], Value::Null);
}

/// Regression: with clavren-update.path stopped nothing ever picked the request up, and the
/// dashboard said "starting" with the Update button off for ever.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_request_nobody_picks_up_fails_and_can_be_sent_again(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({}));
    let reply = updates.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED, "{:?}", reply.body);
    let three_minutes_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(180);
    std::fs::File::options()
        .write(true)
        .open(updates.dir.join("request.json"))
        .unwrap()
        .set_modified(three_minutes_ago)
        .unwrap();

    let body = updates.get().await;
    assert_eq!((&body["update"]["state"], &body["update"]["version"]), (&json!("failed"), &json!(newer())));
    assert!(body["update"]["message"].as_str().unwrap().contains("clavren-update.path"), "{body}");
    assert_eq!(body["can_install"], true);
    let again = updates.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!(again.status, StatusCode::ACCEPTED, "{:?}", again.body);
    assert_eq!(updates.get().await["update"]["state"], "running", "a fresh request starts over");
}
