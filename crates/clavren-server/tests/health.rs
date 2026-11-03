// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:30.255367
//+ Revised: 	2026/11/03 04:54:23.494207
// -----------------------------------------------------------------------------

//! `/health/live` and `/health/ready`: they answer, they report a lost database, and they never
//! say more than up or down.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use clavren::http::router;
use common::{capture_logs, test_state};
use sqlx::PgPool;
use tower::ServiceExt;

async fn get(pool: PgPool, path: &str) -> (StatusCode, String, Option<String>) {
    let (state, _clock) = test_state(pool);
    let response = router(state).oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let request_id = response.headers().get("x-request-id").map(|value| value.to_str().unwrap().to_string());
    let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap(), request_id)
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn live_answers_ok(pool: PgPool) {
    let (status, body, _) = get(pool, "/health/live").await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, r#"{"status":"ok"}"#));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn ready_answers_ok_with_a_database(pool: PgPool) {
    let (status, body, _) = get(pool, "/health/ready").await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, r#"{"status":"ok"}"#));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn ready_reports_a_lost_database_without_details(pool: PgPool) {
    let (logs, _guard) = capture_logs();
    pool.close().await;

    let (status, body, _) = get(pool, "/health/ready").await;

    assert_eq!((status, body.as_str()), (StatusCode::SERVICE_UNAVAILABLE, r#"{"status":"unavailable"}"#));
    let logs = logs.text();
    assert!(logs.contains("WARN") && logs.contains("clavren::http::health"), "{logs}");
}

/// A database that doesn't answer in time (here: every connection busy) counts as unavailable,
/// instead of leaving the health check hanging.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn ready_gives_up_on_a_database_that_does_not_answer(pool: PgPool) {
    let options = (*pool.connect_options()).clone();
    let busy_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect_with(options)
        .await
        .unwrap();
    let _held = busy_pool.acquire().await.unwrap();
    let started = std::time::Instant::now();

    let (status, body, _) = get(busy_pool.clone(), "/health/ready").await;

    assert_eq!((status, body.as_str()), (StatusCode::SERVICE_UNAVAILABLE, r#"{"status":"unavailable"}"#));
    assert!(started.elapsed() < std::time::Duration::from_secs(10), "waited {:?}", started.elapsed());
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn every_response_carries_a_request_id(pool: PgPool) {
    let (_, _, request_id) = get(pool.clone(), "/health/live").await;
    let (_, _, another) = get(pool, "/health/live").await;
    let (request_id, another) = (request_id.unwrap(), another.unwrap());
    assert_eq!(request_id.len(), 36, "{request_id}");
    assert_ne!(request_id, another);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn unknown_paths_are_not_found(pool: PgPool) {
    let (status, _, _) = get(pool, "/health/secrets").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn request_logs_have_the_path_but_never_the_query(pool: PgPool) {
    let (logs, _guard) = capture_logs();
    get(pool, "/health/live?q=BRN-7KPX-4M9D-QW2R-8NFC").await;
    let logs = logs.text();
    assert!(logs.contains("/health/live"), "{logs}");
    assert!(!logs.contains("BRN-7KPX"), "{logs}");
}
