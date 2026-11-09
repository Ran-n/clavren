// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.753452
//+ Revised: 	2026/11/09 10:00:42.469994
// -----------------------------------------------------------------------------

//! Backups: a `pg_dump` archive, as `deploy/clavren-backup.sh` makes it, restores into an empty
//! database, and with the same master key the licences, activations and keys still work. Also
//! checks the deploy scripts parse.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::admin::PASSWORD;
use common::api::{Call, client, sign_in};
use common::device::Device;
use common::licensing::{Licensing, PRODUCT};
use common::{test_app, test_state};
use serde_json::json;
// The database names are made here or by sqlx::test, never from input.
use sqlx::{AssertSqlSafe, Connection, PgConnection, PgPool};

/// A PostgreSQL client program: from `PATH`, or the newest Windows install.
fn pg_tool(name: &str) -> PathBuf {
    let on_path = Command::new(name).arg("--version").output().is_ok_and(|output| output.status.success());
    if on_path {
        return PathBuf::from(name);
    }
    let mut installed: Vec<PathBuf> = std::fs::read_dir(r"C:\Program Files\PostgreSQL")
        .into_iter()
        .flatten()
        .flatten()
        .map(|version| version.path().join("bin").join(format!("{name}.exe")))
        .filter(|path| path.exists())
        .collect();
    installed.sort();
    installed.pop().unwrap_or_else(|| panic!("{name} not found: put PostgreSQL's bin folder on PATH"))
}

fn run(program: &Path, args: &[&str]) {
    let output = Command::new(program).args(args).output().unwrap();
    assert!(output.status.success(), "{} failed: {}", program.display(), String::from_utf8_lossy(&output.stderr));
}

/// The URL of database `name` in the test cluster.
fn database_url(name: &str) -> String {
    let server = std::env::var("DATABASE_URL").unwrap();
    let (server, _) = server.rsplit_once('/').unwrap();
    format!("{server}/{name}")
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_backup_restores_into_an_empty_database(pool: PgPool) {
    let licensing = Licensing::new(pool.clone()).await;
    let (licence, key) = licensing.issue(json!({ "customer": { "name": "Ana López" } })).await;
    let device = Device::new(1);
    let activation =
        device.activate(licensing.app(), PRODUCT, &key).await.body["activation"].as_str().unwrap().to_string();

    let source: String = sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await.unwrap();
    let archive = tempfile::tempdir().unwrap();
    let dump = archive.path().join("clavren.dump");
    run(
        &pg_tool("pg_dump"),
        &["--format=custom", "--dbname", &database_url(&source), "--file", dump.to_str().unwrap()],
    );

    // Not derived from `source`: PostgreSQL cuts names at 63 bytes, and sqlx's are already 63.
    let restored = format!("clavren_restored_{}", uuid::Uuid::new_v4().simple());
    let mut server = PgConnection::connect(&database_url("postgres")).await.unwrap();
    sqlx::query(AssertSqlSafe(format!("CREATE DATABASE \"{restored}\""))).execute(&mut server).await.unwrap();
    run(
        &pg_tool("pg_restore"),
        &["--no-owner", "--exit-on-error", "--dbname", &database_url(&restored), dump.to_str().unwrap()],
    );

    let restored_pool = PgPool::connect(&database_url(&restored)).await.unwrap();
    let (state, clock) = test_state(restored_pool.clone());
    let app = test_app(state, client());
    clock.advance(Duration::from_secs(300)); // a TOTP step the original server hasn't used

    assert_eq!(device.check(&app, &activation).await.status, StatusCode::OK, "the computer still checks in");
    let browser = sign_in(&app, "ran", PASSWORD, &licensing.admin.fixture.admin.code_now(&clock)).await;
    let revealed = Call::new(Method::POST, &format!("/api/admin/licences/{licence}/reveal"))
        .signed_in(&browser)
        .json(json!({}))
        .send(&app)
        .await;
    assert_eq!(revealed.body["key"], key.as_str(), "the key decrypts with the same master key");
    let read = Call::new(Method::GET, &format!("/api/admin/licences/{licence}?expand=customer"))
        .signed_in(&browser)
        .send(&app)
        .await;
    assert_eq!(
        (read.body["state"].as_str(), read.body["customer"]["name"].as_str()),
        (Some("redeemed"), Some("Ana López"))
    );

    restored_pool.close().await;
    sqlx::query(AssertSqlSafe(format!("DROP DATABASE \"{restored}\" WITH (FORCE)")))
        .execute(&mut server)
        .await
        .unwrap();
}

#[test]
fn the_deploy_scripts_parse() {
    let deploy = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy");
    for script in [
        "clavren-backup.sh",
        "clavren-restore.sh",
        "install.sh",
        "clavren-new-admin.sh",
        "clavren-update-check.sh",
        "clavren-update.sh",
    ] {
        // On stdin: bash on Windows would read a backslashed path as escapes.
        let source = std::fs::File::open(deploy.join(script)).unwrap();
        let output = Command::new("bash").arg("-n").stdin(source).output().expect("bash is installed");
        assert!(output.status.success(), "{script}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
