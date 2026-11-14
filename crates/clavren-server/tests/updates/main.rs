// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/09 09:57:45.846323
//+ Revised: 	2026/11/14 12:05:28.491086
// -----------------------------------------------------------------------------

//! `/api/admin/updates`: what the update scripts left in the data folder, as the dashboard sees
//! it, and the requests the server writes for them.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod checking;
#[path = "../common/mod.rs"]
mod common;
mod installing;

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
