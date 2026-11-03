// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.529843
//+ Revised: 	2026/11/03 11:44:04.517819
// -----------------------------------------------------------------------------

//! The server the dashboard's end-to-end tests drive (`web/admin/e2e`): a fresh
//! database in the local test cluster, a fresh data directory, and one admin created through the
//! real `create-admin` command. Prints one JSON line with the address, the admin's TOTP secret
//! and recovery codes, then serves until stdin closes or Ctrl+C.
//!
//! The server's clock stands still at [`FROZEN_AT`], so every date on screen is the same in every
//! run and the screenshots never change. The admin is created an hour earlier, also on a frozen
//! clock, so the authenticator codes for the server's time and the steps either side of it all
//! still work once.
//!
//! `cargo run --example e2e_server -- <port> <data dir> <database>`; `DATABASE_URL` names the test
//! cluster. The database (lowercase letters, digits and `_`) is dropped and made again.

use std::io::{self, BufRead};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::TimeDelta;
use chrono::{DateTime, Utc};
use clavren::admin_auth::totp;
use clavren::admin_cli::terminal::Terminal;
use clavren::admin_cli::{self, CliContext, Command};
use clavren::clock::ManualClock;
use clavren::config::Config;
use clavren::startup::{self, Server};
use sqlx::{AssertSqlSafe, Connection, PgConnection};

const USERNAME: &str = "e2e";
const PASSWORD: &str = "e2e password, long enough";
const SECRET_MARKER: &str = "enter this key by hand: ";
/// On a 30-second TOTP step boundary.
const FROZEN_AT: &str = "2027-03-01T09:30:00Z";

/// Answers `create-admin` like a person with an authenticator app would.
struct Enrolment {
    output: String,
    at: DateTime<Utc>,
}

impl Terminal for Enrolment {
    fn say(&mut self, text: &str) {
        self.output.push_str(text);
        self.output.push('\n');
    }

    fn ask(&mut self, _prompt: &str) -> io::Result<String> {
        let secret = self.secret().ok_or_else(|| io::Error::other("create-admin printed no secret"))?;
        let bytes = totp::secret_from_text(&secret).ok_or_else(|| io::Error::other("unreadable secret"))?;
        totp::code_at(&bytes, USERNAME, self.at).map_err(io::Error::other)
    }

    fn ask_secret(&mut self, _prompt: &str) -> io::Result<String> {
        Ok(PASSWORD.to_string())
    }
}

impl Enrolment {
    fn secret(&self) -> Option<String> {
        let start = self.output.rfind(SECRET_MARKER)? + SECRET_MARKER.len();
        self.output[start..].lines().next().map(str::to_string)
    }

    fn recovery_codes(&self) -> Vec<&str> {
        self.output.lines().map(str::trim).filter(|line| line.len() == 31 && line.matches('-').count() == 5).collect()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    const USAGE: &str = "usage: e2e_server <port> <data dir> <database>";
    let port: u16 = args.next().ok_or(USAGE)?.parse()?;
    let data_dir = PathBuf::from(args.next().ok_or(USAGE)?);
    let database = args.next().ok_or(USAGE)?;
    if database.is_empty()
        || !database.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(USAGE.into());
    }
    let cluster = std::env::var("DATABASE_URL")?;
    let (server_url, _) = cluster.rsplit_once('/').ok_or("DATABASE_URL has no database name")?;

    let mut admin_connection = PgConnection::connect(&cluster).await?;
    // The name was checked above: lowercase letters, digits and underscores only.
    sqlx::query(AssertSqlSafe(format!("DROP DATABASE IF EXISTS {database} WITH (FORCE)")))
        .execute(&mut admin_connection)
        .await?;
    sqlx::query(AssertSqlSafe(format!("CREATE DATABASE {database}"))).execute(&mut admin_connection).await?;
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir)?;
    }

    let address = format!("127.0.0.1:{port}");
    let settings = [
        ("CLAVREN_DATABASE_URL", format!("{server_url}/{database}")),
        ("CLAVREN_PUBLIC_URL", format!("http://{address}")),
        ("CLAVREN_LISTEN", address.clone()),
        ("CLAVREN_DATA_DIR", data_dir.display().to_string()),
        // Every E2E sign-in comes from one address; the limit has its own integration tests.
        ("CLAVREN_LOGIN_MAX_FAILURES", "1000".to_string()),
    ];
    let config =
        Config::from_lookup(|name| settings.iter().find(|(key, _)| *key == name).map(|(_, value)| value.clone()))?;

    let (pool, keys) = startup::open(&config).await?;
    let frozen_at: DateTime<Utc> = FROZEN_AT.parse()?;
    let enrolled_at = frozen_at - TimeDelta::hours(1);
    let clock = Arc::new(ManualClock::new(enrolled_at));
    let context = CliContext { pool: pool.clone(), keys, clock, signing_key_version: 1 };
    let mut enrolment = Enrolment { output: String::new(), at: enrolled_at };
    admin_cli::run(&Command::CreateAdmin { username: USERNAME.into() }, &context, &mut enrolment).await?;
    pool.close().await;

    let server = Server::prepare_with_clock(&config, Arc::new(ManualClock::new(frozen_at))).await?;
    let ready = serde_json::json!({
        "url": format!("http://{address}"),
        "username": USERNAME,
        "password": PASSWORD,
        "secret": enrolment.secret(),
        "recovery_codes": enrolment.recovery_codes(),
        "frozen_at": FROZEN_AT,
    });
    println!("{ready}");
    server.serve(stdin_closed()).await?;
    Ok(())
}

/// Completes when the test runner closes stdin (or exits), or on Ctrl+C.
async fn stdin_closed() {
    let closed = tokio::task::spawn_blocking(|| for _ in io::stdin().lock().lines() {});
    tokio::select! {
        _ = closed => {}
        _ = startup::shutdown_signal() => {}
    }
}
