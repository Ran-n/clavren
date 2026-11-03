// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:30.196162
//+ Revised: 	2026/11/03 04:54:25.351155
// -----------------------------------------------------------------------------

//! Starting the server against a real database: the master key is created only when nothing
//! depends on it, an existing one is reused, the server answers on its port, and shutting down
//! closes the database pool. Secrets never reach the log.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use clavren::crypto::master_key::{self, MasterKeyFile};
use clavren::startup::{Server, StartupError};
use common::{capture_logs, http_get, test_config};
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use tokio::sync::oneshot;

fn key_bytes(data_dir: &std::path::Path) -> Vec<u8> {
    match master_key::read(data_dir).unwrap() {
        MasterKeyFile::Found(key) => key.as_bytes().to_vec(),
        MasterKeyFile::Missing => panic!("no master key in {}", data_dir.display()),
    }
}

#[sqlx::test(migrations = false)]
async fn first_start_creates_the_master_key_and_migrates(_pool: PgPoolOptions, options: PgConnectOptions) {
    let data_dir = tempfile::tempdir().unwrap();
    let (logs, _guard) = capture_logs();

    Server::prepare(&test_config(options.clone(), data_dir.path())).await.unwrap();

    assert_eq!(key_bytes(data_dir.path()).len(), 32);
    let pool = PgPool::connect_with(options).await.unwrap();
    let tables: i64 =
        sqlx::query_scalar("SELECT count(*) FROM information_schema.tables WHERE table_name = 'licenses'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tables, 1);
    let logs = logs.text();
    assert!(logs.contains("WARN") && logs.contains("created a new master key"), "{logs}");
}

#[sqlx::test(migrations = false)]
async fn a_later_start_reuses_the_existing_key(_pool: PgPoolOptions, options: PgConnectOptions) {
    let data_dir = tempfile::tempdir().unwrap();
    Server::prepare(&test_config(options.clone(), data_dir.path())).await.unwrap();
    let first = key_bytes(data_dir.path());

    Server::prepare(&test_config(options, data_dir.path())).await.unwrap();

    assert_eq!(key_bytes(data_dir.path()), first);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_missing_key_is_not_replaced_when_licences_depend_on_it(pool: PgPool) {
    let product: uuid::Uuid = sqlx::query_scalar("INSERT INTO products (code, name) VALUES ('p', 'P') RETURNING id")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO licenses (product_id, key_lookup) VALUES ($1, $2)")
        .bind(product)
        .bind([9u8; 32].as_slice())
        .execute(&pool)
        .await
        .unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let options = (*pool.connect_options()).clone();

    let result = Server::prepare(&test_config(options, data_dir.path())).await;

    let Err(error) = result else { panic!("started without its master key") };
    assert!(matches!(error, StartupError::MasterKey(master_key::MasterKeyError::MissingWithEncryptedData { .. })));
    assert!(error.to_string().contains("restore master.key"), "{error}");
    assert!(matches!(master_key::read(data_dir.path()).unwrap(), MasterKeyFile::Missing), "a new key was written");
}

#[sqlx::test(migrations = false)]
async fn an_unreachable_database_is_reported_clearly(_pool: PgPoolOptions, options: PgConnectOptions) {
    let data_dir = tempfile::tempdir().unwrap();
    let unreachable = options.port(1);

    let Err(error) = Server::prepare(&test_config(unreachable, data_dir.path())).await else {
        panic!("started without a database")
    };

    assert!(error.to_string().contains("CLAVREN_DATABASE_URL"), "{error}");
}

#[sqlx::test(migrations = false)]
async fn a_data_dir_that_cannot_be_created_is_reported(_pool: PgPoolOptions, options: PgConnectOptions) {
    let parent = tempfile::tempdir().unwrap();
    let not_a_dir = parent.path().join("a-file");
    std::fs::write(&not_a_dir, b"").unwrap();

    let Err(error) = Server::prepare(&test_config(options, &not_a_dir.join("clavren"))).await else {
        panic!("started without a usable data directory")
    };

    assert!(matches!(error, StartupError::MasterKey(master_key::MasterKeyError::Io { .. })), "{error}");
    assert!(error.to_string().contains("a-file"), "{error}");
}

#[sqlx::test(migrations = false)]
async fn an_address_already_in_use_is_reported(_pool: PgPoolOptions, options: PgConnectOptions) {
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let mut config = test_config(options, data_dir.path());
    config.listen_address = occupied.local_addr().unwrap();

    let Err(error) = Server::prepare(&config).await else { panic!("listened on an occupied port") };

    assert!(matches!(error, StartupError::Listen { .. }));
}

#[sqlx::test(migrations = false)]
async fn the_server_answers_then_shuts_down_cleanly(_pool: PgPoolOptions, options: PgConnectOptions) {
    let data_dir = tempfile::tempdir().unwrap();
    let (logs, _guard) = capture_logs();
    let server = Server::prepare(&test_config(options, data_dir.path())).await.unwrap();
    let address = server.local_address().unwrap();
    let (stop, stopped) = oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve(async {
        let _ = stopped.await;
    }));

    let (status, body) = http_get(address, "/health/ready").await;
    assert_eq!(status, 200, "{body}");
    assert!(body.to_ascii_lowercase().contains("x-request-id"), "{body}");

    stop.send(()).unwrap();
    serving.await.unwrap().unwrap();
    assert!(tokio::net::TcpStream::connect(address).await.is_err(), "still listening after shutdown");
    let logs = logs.text();
    assert!(logs.contains("closing the database pool") && logs.contains("stopped"), "{logs}");
}

#[sqlx::test(migrations = false)]
async fn startup_logs_contain_no_secrets(_pool: PgPoolOptions, options: PgConnectOptions) {
    let data_dir = tempfile::tempdir().unwrap();
    let (logs, _guard) = capture_logs();
    let options = options.password("pa55-not-for-logs");

    // the password is wrong for the trust-auth test cluster only if it checks; either way it must not be logged
    let _ = Server::prepare(&test_config(options, data_dir.path())).await;

    let logs = logs.text();
    assert!(!logs.contains("pa55-not-for-logs"), "{logs}");
    if let Ok(MasterKeyFile::Found(key)) = master_key::read(data_dir.path()) {
        let hex: String = key.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect();
        assert!(!logs.contains(&hex), "the master key was logged");
    }
}
