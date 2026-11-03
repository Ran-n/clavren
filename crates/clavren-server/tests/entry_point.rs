// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:30.312147
//+ Revised: 	2026/11/03 04:54:23.429909
// -----------------------------------------------------------------------------

//! The `clavren` and `clavren-admin` binaries themselves: they fail with a readable message and a
//! non-zero exit code when they can't start, and work when they can.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use std::net::SocketAddr;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::http_get;

fn clavren() -> Command {
    binary(env!("CARGO_BIN_EXE_clavren"))
}

fn clavren_admin() -> Command {
    binary(env!("CARGO_BIN_EXE_clavren-admin"))
}

/// The binary with no `CLAVREN_*` variables from the environment.
fn binary(path: &str) -> Command {
    let mut command = Command::new(path);
    for (name, _) in std::env::vars().filter(|(name, _)| name.starts_with("CLAVREN_")) {
        command.env_remove(name);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    command
}

#[test]
fn missing_configuration_fails_naming_the_variable() {
    let output = clavren().output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(printed.contains("CLAVREN_DATABASE_URL is not set"), "{printed}");
}

#[test]
fn an_invalid_log_filter_fails_on_stderr() {
    let output = clavren().env("CLAVREN_LOG", "info,clavren=loud").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let printed = String::from_utf8_lossy(&output.stderr);
    assert!(printed.starts_with("clavren: CLAVREN_LOG"), "{printed}");
}

#[test]
fn an_unreachable_database_fails_without_showing_the_password() {
    let output = clavren()
        .env("CLAVREN_DATABASE_URL", "postgres://clavren:pa55word@127.0.0.1:1/clavren")
        .env("CLAVREN_PUBLIC_URL", "http://localhost")
        .env("CLAVREN_DATA_DIR", tempfile::tempdir().unwrap().path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(printed.contains("could not connect to the database"), "{printed}");
    assert!(!printed.contains("pa55word"), "{printed}");
}

/// Starts the real binary against the test database (a fresh one, created here) and waits for it
/// to answer on its health endpoint.
#[sqlx::test(migrations = false)]
async fn the_binary_serves_with_a_database(
    _pool: sqlx::postgres::PgPoolOptions,
    options: sqlx::postgres::PgConnectOptions,
) {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let url = database_url(&options);
    let data_dir = tempfile::tempdir().unwrap();
    let mut child = clavren()
        .env("CLAVREN_DATABASE_URL", url)
        .env("CLAVREN_PUBLIC_URL", "http://localhost")
        .env("CLAVREN_LISTEN", format!("127.0.0.1:{port}"))
        .env("CLAVREN_DATA_DIR", data_dir.path())
        .spawn()
        .unwrap();

    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline && tokio::net::TcpStream::connect(address).await.is_err() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let ready = match tokio::net::TcpStream::connect(address).await {
        Ok(_) => Some(http_get(address, "/health/ready").await),
        Err(_) => None,
    };
    child.kill().unwrap();
    let output = child.wait_with_output().unwrap();
    let (status, body) = ready.unwrap_or_else(|| panic!("never listened: {}", String::from_utf8_lossy(&output.stdout)));
    assert_eq!(status, 200, "{body}");
    assert!(data_dir.path().join("master.key").is_file());
}

fn database_url(options: &sqlx::postgres::PgConnectOptions) -> String {
    let database = options.get_database().unwrap_or("postgres");
    format!("postgres://{}@{}:{}/{}", options.get_username(), options.get_host(), options.get_port(), database)
}

#[test]
fn the_admin_tool_explains_itself() {
    let output = clavren_admin().arg("--help").output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let printed = String::from_utf8_lossy(&output.stdout);
    for command in ["create-admin", "reset-password", "disable-admin", "enable-admin", "list-admins"] {
        assert!(printed.contains(command), "{command} missing from {printed}");
    }
}

#[test]
fn the_admin_tool_rejects_unknown_commands_before_touching_anything() {
    let output = clavren_admin().arg("drop-everything").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("drop-everything"));
}

#[test]
fn the_admin_tool_reports_an_invalid_log_filter() {
    let output = clavren_admin().arg("list-admins").env("CLAVREN_LOG", "warn,clavren=loud").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let printed = String::from_utf8_lossy(&output.stderr);
    assert!(printed.starts_with("clavren-admin: CLAVREN_LOG"), "{printed}");
}

#[test]
fn the_admin_tool_needs_the_server_configuration() {
    let output = clavren_admin().arg("list-admins").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let printed = String::from_utf8_lossy(&output.stderr);
    assert!(printed.starts_with("clavren-admin: CLAVREN_DATABASE_URL is not set"), "{printed}");
}

#[sqlx::test(migrations = false)]
async fn the_admin_tool_works_on_a_fresh_database(
    _pool: sqlx::postgres::PgPoolOptions,
    options: sqlx::postgres::PgConnectOptions,
) {
    let data_dir = tempfile::tempdir().unwrap();
    let admin = |arguments: &[&str]| {
        clavren_admin()
            .args(arguments)
            .env("CLAVREN_DATABASE_URL", database_url(&options))
            .env("CLAVREN_PUBLIC_URL", "http://localhost")
            .env("CLAVREN_DATA_DIR", data_dir.path())
            .output()
            .unwrap()
    };

    let output = admin(&["list-admins"]);
    assert_eq!(output.status.code(), Some(0), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("No admins yet"));
    assert!(data_dir.path().join("master.key").is_file(), "the first run creates the master key");

    let output = admin(&["disable-admin", "ghost"]);
    assert_eq!(output.status.code(), Some(1));
    let printed = String::from_utf8_lossy(&output.stderr);
    assert!(printed.starts_with("clavren-admin: ") && printed.contains("ghost"), "{printed}");
}
