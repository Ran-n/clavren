// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:30.025571
//+ Revised: 	2026/11/09 10:00:42.341591
// -----------------------------------------------------------------------------

//! Helpers shared by the integration tests: a config for a test database, log capture, and a
//! minimal HTTP client for a server listening on a real port.

#![allow(dead_code)] // each test file uses a different subset

use std::cell::RefCell;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex, Once};

use axum::Router;
use axum::extract::connect_info::MockConnectInfo;
use chrono::{DateTime, Utc};
use clavren::clock::{Clock, ManualClock};
use clavren::config::{AdminAuthSettings, Config, LicensingSettings, ProxySettings, UpdateSettings};
use clavren::crypto::key_ring::KeyRing;
use clavren::crypto::master_key::MasterKey;
use clavren::http::{AppState, Settings, router};
use sqlx::PgPool;
use sqlx::postgres::PgConnectOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing_subscriber::fmt::MakeWriter;

pub const TEST_MASTER_KEY: [u8; 32] = [1; 32];

pub fn test_config(database: PgConnectOptions, data_dir: &Path) -> Config {
    Config {
        database,
        listen_address: "127.0.0.1:0".parse().unwrap(),
        data_dir: data_dir.to_path_buf(),
        public_url: "http://localhost:8080".into(),
        proxies: ProxySettings::default(),
        admin: AdminAuthSettings { secure_cookies: false, ..AdminAuthSettings::default() },
        licensing: LicensingSettings::default(),
        updates: UpdateSettings { dir: data_dir.join("updates"), ..UpdateSettings::default() },
    }
}

/// A fixed moment, well inside a TOTP step, so codes made "now" stay valid for the test.
pub fn test_start() -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_010, 0).unwrap()
}

pub fn test_settings() -> Settings {
    Settings {
        public_url: "https://licences.example.com".into(),
        proxies: ProxySettings::default(),
        admin: AdminAuthSettings::default(),
        licensing: LicensingSettings::default(),
        updates: UpdateSettings::default(),
    }
}

/// Server state on `pool` with a clock the test moves, and the master key `TEST_MASTER_KEY`.
pub fn test_state(pool: PgPool) -> (AppState, Arc<ManualClock>) {
    test_state_with(pool, test_settings())
}

/// [`test_state`] with other settings.
pub fn test_state_with(pool: PgPool, settings: Settings) -> (AppState, Arc<ManualClock>) {
    let clock = Arc::new(ManualClock::new(test_start()));
    let shared: Arc<dyn Clock> = clock.clone();
    let state = AppState::new(pool, KeyRing::new(MasterKey::from_bytes(TEST_MASTER_KEY)), shared, settings);
    (state, clock)
}

/// The full router, as if every request came from `client` (no proxy involved).
pub fn test_app(state: AppState, client: SocketAddr) -> Router {
    router(state).layer(MockConnectInfo(client))
}

/// Everything logged on this thread while the guard lives, as plain text.
#[derive(Clone, Default)]
pub struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

impl CapturedLogs {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

thread_local! {
    static CAPTURE: RefCell<Option<CapturedLogs>> = const { RefCell::new(None) };
}

/// Writes to the capture of the thread that logged, if it has one; otherwise nowhere.
pub struct ThreadCapture;

impl std::io::Write for ThreadCapture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        CAPTURE.with_borrow(|capture| {
            if let Some(logs) = capture {
                logs.0.lock().unwrap().extend_from_slice(bytes);
            }
        });
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for ThreadCapture {
    type Writer = ThreadCapture;

    fn make_writer(&'a self) -> Self::Writer {
        ThreadCapture
    }
}

/// Stops capturing when dropped.
pub struct CaptureGuard;

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURE.set(None);
    }
}

/// Captures what this thread logs. `sqlx::test` runs each test on a current-thread runtime, so
/// that includes the tasks the test spawns.
///
/// One global subscriber does the work rather than a scoped one per test: scoped subscribers race
/// with tests on other threads over tracing's shared interest and max-level caches, and lose
/// lines now and then, which would make every "this is never logged" check worthless.
pub fn capture_logs() -> (CapturedLogs, CaptureGuard) {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_writer(ThreadCapture)
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_target(true)
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect("another global subscriber is installed");
    });
    let logs = CapturedLogs::default();
    CAPTURE.set(Some(logs.clone()));
    (logs, CaptureGuard)
}

/// `GET path` over a fresh connection; returns the status code and the whole response text.
pub async fn http_get(address: SocketAddr, path: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response.split_whitespace().nth(1).and_then(|code| code.parse().ok()).unwrap_or(0);
    (status, response)
}

pub mod admin;
pub mod api;
pub mod device;
pub mod licensing;
