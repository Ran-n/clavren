// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.005194
//+ Revised: 	2026/11/07 19:36:59.297085
// -----------------------------------------------------------------------------

//! The client against the real server over HTTP, the way avalio-lite will use it: activate, check,
//! reset and activate on a second computer (the first locks), revoke (the second locks), and a
//! deleted licence or product (its computer locks).

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

use std::sync::Arc;
use std::time::Duration;

use clavren::activations::tokens;
use clavren::config::{AdminAuthSettings, Config, LicensingSettings, ProxySettings};
use clavren::crypto::key_ring::KeyRing;
use clavren::crypto::master_key::{self, MasterKeyFile};
use clavren::licences::{repository, secrets};
use clavren::startup::Server;
use clavren_client::clock::SystemClock;
use clavren_client::store::MemoryStore;
use clavren_client::transport::HttpTransport;
use clavren_client::{Client, Outcome, trusted_keys};
use clavren_protocol::encoding;
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use uuid::Uuid;

const PRODUCT: &str = "avalio-lite";

struct Running {
    url: String,
    pool: PgPool,
    keys: KeyRing,
    stop: tokio::sync::oneshot::Sender<()>,
    serving: tokio::task::JoinHandle<()>,
    _data_dir: tempfile::TempDir,
}

async fn start(options: PgConnectOptions) -> Running {
    let data_dir = tempfile::tempdir().unwrap();
    let config = Config {
        database: options.clone(),
        listen_address: "127.0.0.1:0".parse().unwrap(),
        data_dir: data_dir.path().to_path_buf(),
        public_url: "http://localhost".into(),
        proxies: ProxySettings::default(),
        admin: AdminAuthSettings { secure_cookies: false, ..AdminAuthSettings::default() },
        licensing: LicensingSettings::default(),
    };
    let server = Server::prepare(&config).await.unwrap();
    let url = format!("http://{}", server.local_address().unwrap());
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(async move {
        server.serve(async { stopped.await.unwrap_or_default() }).await.unwrap();
    });
    let MasterKeyFile::Found(master) = master_key::read(data_dir.path()).unwrap() else { panic!("no master key") };
    let pool = PgPoolOptions::new().connect_with(options).await.unwrap();
    Running { url, pool, keys: KeyRing::new(master), stop, serving, _data_dir: data_dir }
}

/// Issues a licence the way the admin API does (hash and encrypted key); returns its ID and key.
async fn issue(running: &Running) -> (Uuid, String) {
    let product: Uuid = sqlx::query_scalar(
        "INSERT INTO products (code, name) VALUES ($1, 'Avalio Lite') ON CONFLICT (code) DO UPDATE SET name = 'Avalio Lite' RETURNING id",
    )
    .bind(PRODUCT)
    .fetch_one(&running.pool)
    .await
    .unwrap();
    let (licence, key) = (Uuid::now_v7(), secrets::generate().unwrap());
    let sealed = secrets::seal(&running.keys, &key, licence, product).unwrap();
    sqlx::query("INSERT INTO licenses (id, product_id, key_lookup) VALUES ($1, $2, $3)")
        .bind(licence)
        .bind(product)
        .bind(secrets::lookup_hash(&running.keys, &key).as_slice())
        .execute(&running.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO license_key_secrets (license_id, ciphertext, nonce, encryption_version) VALUES ($1, $2, $3, 1)",
    )
    .bind(licence)
    .bind(sealed.ciphertext)
    .bind(sealed.nonce.to_vec())
    .execute(&running.pool)
    .await
    .unwrap();
    (licence, key.formatted())
}

fn computer(running: &Running) -> Arc<Client> {
    let public_key = encoding::encode(tokens::public_key(&running.keys, 1).as_bytes());
    let keys = trusted_keys(&[("v1", &public_key)]).unwrap();
    let config = clavren_client::Config::new(PRODUCT, keys, "1.0.0");
    Arc::new(Client::new(
        config,
        Box::new(MemoryStore::default()),
        Box::new(HttpTransport::new(&running.url)),
        Box::new(SystemClock),
    ))
}

/// Runs the blocking client off the async runtime the server is on.
async fn on_computer<T: Send + 'static>(client: &Arc<Client>, action: impl FnOnce(&Client) -> T + Send + 'static) -> T {
    let client = client.clone();
    tokio::task::spawn_blocking(move || action(&client)).await.unwrap()
}

#[sqlx::test(migrations = false)]
async fn a_licence_moves_between_computers_and_ends(_pool: PgPoolOptions, options: PgConnectOptions) {
    let running = start(options).await;
    let (licence, key) = issue(&running).await;
    let (first, second) = (computer(&running), computer(&running));

    let typed = key.to_lowercase();
    assert!(on_computer(&first, move |client| client.activate(&typed).unwrap()).await.is_licensed());
    assert!(on_computer(&first, |client| client.check().unwrap()).await.is_licensed());
    let key_again = key.clone();
    let refused = on_computer(&second, move |client| client.activate(&key_again).unwrap()).await;
    assert_eq!(refused, Outcome::AlreadyActivatedElsewhere);

    // the administrator resets the licence for the customer's new computer
    sqlx::query("UPDATE activations SET revoked_at = now(), revocation_reason = 'reset' WHERE license_id = $1 AND revoked_at IS NULL")
        .bind(licence)
        .execute(&running.pool)
        .await
        .unwrap();
    let key_again = key.clone();
    assert!(on_computer(&second, move |client| client.activate(&key_again).unwrap()).await.is_licensed());
    assert_eq!(on_computer(&first, |client| client.check().unwrap()).await, Outcome::Revoked, "the old computer locks");

    // and later revokes it
    sqlx::query("UPDATE licenses SET revoked_at = now() WHERE id = $1")
        .bind(licence)
        .execute(&running.pool)
        .await
        .unwrap();
    assert_eq!(on_computer(&second, |client| client.check().unwrap()).await, Outcome::Revoked);
    assert_eq!(on_computer(&second, |client| client.status()).await, Outcome::Revoked);

    running.stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(30), running.serving).await.unwrap().unwrap();
}

#[sqlx::test(migrations = false)]
async fn a_deleted_licences_computer_locks(_pool: PgPoolOptions, options: PgConnectOptions) {
    let running = start(options).await;
    let (licence, key) = issue(&running).await;
    let client = computer(&running);
    assert!(on_computer(&client, move |client| client.activate(&key).unwrap()).await.is_licensed());

    let mut connection = running.pool.acquire().await.unwrap();
    repository::delete(&mut connection, &[licence]).await.unwrap();
    assert_eq!(on_computer(&client, |client| client.check().unwrap()).await, Outcome::Revoked, "a definitive refusal");
    assert_eq!(on_computer(&client, |client| client.status()).await, Outcome::Revoked, "the token is gone");

    running.stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(30), running.serving).await.unwrap().unwrap();
}

#[sqlx::test(migrations = false)]
async fn a_deleted_products_computer_locks(_pool: PgPoolOptions, options: PgConnectOptions) {
    let running = start(options).await;
    let (licence, key) = issue(&running).await;
    let client = computer(&running);
    assert!(on_computer(&client, move |client| client.activate(&key).unwrap()).await.is_licensed());

    let product: Uuid = sqlx::query_scalar("SELECT product_id FROM licenses WHERE id = $1")
        .bind(licence)
        .fetch_one(&running.pool)
        .await
        .unwrap();
    let mut transaction = running.pool.begin().await.unwrap();
    let ids = repository::ids_of_product(&mut transaction, product).await.unwrap();
    repository::delete(&mut transaction, &ids).await.unwrap();
    clavren::products::repository::delete(&mut *transaction, product).await.unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(on_computer(&client, |client| client.check().unwrap()).await, Outcome::Revoked, "a definitive refusal");
    assert_eq!(on_computer(&client, |client| client.status()).await, Outcome::Revoked, "the token is gone");

    running.stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(30), running.serving).await.unwrap().unwrap();
}

#[sqlx::test(migrations = false)]
async fn a_wrong_or_unreachable_server_never_unlocks_anything(_pool: PgPoolOptions, options: PgConnectOptions) {
    let running = start(options).await;
    let (_, key) = issue(&running).await;
    let client = computer(&running);
    let typed = "BRN-0000-0000-0000-0000".to_string();
    assert_eq!(on_computer(&client, move |client| client.activate(&typed).unwrap()).await, Outcome::InvalidKey);

    let nowhere = {
        let public_key = encoding::encode(tokens::public_key(&running.keys, 1).as_bytes());
        let config = clavren_client::Config::new(PRODUCT, trusted_keys(&[("v1", &public_key)]).unwrap(), "1.0.0");
        let transport = HttpTransport::new("http://127.0.0.1:9");
        Arc::new(Client::new(config, Box::new(MemoryStore::default()), Box::new(transport), Box::new(SystemClock)))
    };
    assert_eq!(on_computer(&nowhere, move |client| client.activate(&key).unwrap()).await, Outcome::Unreachable);
    running.stop.send(()).unwrap();
}
