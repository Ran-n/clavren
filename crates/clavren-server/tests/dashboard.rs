// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:17.000102
//+ Revised: 	2026/11/09 22:30:56.099548
// -----------------------------------------------------------------------------

//! The embedded dashboard: its files, `index.html` for every page path, security and cache
//! headers, and API paths that stay JSON errors.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use clavren::dashboard::ASSETS;
use common::api::client;
use common::{test_app, test_state};
use sqlx::PgPool;
use tower::ServiceExt;

struct Answer {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Vec<u8>,
}

async fn send(app: &Router, method: Method, path: &str) -> Answer {
    let request = Request::builder().method(method).uri(path).body(Body::empty()).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    Answer { status: parts.status, headers: parts.headers, body: to_bytes(body, 8 << 20).await.unwrap().to_vec() }
}

fn app(pool: PgPool) -> Router {
    test_app(test_state(pool).0, client())
}

fn header(answer: &Answer, name: header::HeaderName) -> &str {
    answer.headers.get(name).map(|value| value.to_str().unwrap()).unwrap_or_default()
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn every_page_path_gets_the_dashboard(pool: PgPool) {
    let app = app(pool);
    let root = send(&app, Method::GET, "/").await;
    let built = ASSETS.iter().any(|(path, _)| *path == "index.html");
    assert_eq!(root.status, if built { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE });
    assert_eq!(header(&root, header::CONTENT_TYPE), "text/html; charset=utf-8");
    assert!(header(&root, header::CONTENT_SECURITY_POLICY).contains("frame-ancestors 'none'"));
    assert_eq!(header(&root, header::X_FRAME_OPTIONS), "DENY");
    assert_eq!(header(&root, header::CACHE_CONTROL), "no-cache", "a new build must show at once");

    for page in ["/licences", "/licences/0190a3c2-0000-7000-8000-000000000000", "/account"] {
        let answer = send(&app, Method::GET, page).await;
        assert_eq!((answer.status, &answer.body), (root.status, &root.body), "{page}");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn built_files_are_served_and_cached_for_good(pool: PgPool) {
    let app = app(pool);
    for (path, bytes) in ASSETS.iter().filter(|(path, _)| path.starts_with("assets/")) {
        let answer = send(&app, Method::GET, &format!("/{path}")).await;
        assert_eq!(answer.status, StatusCode::OK, "{path}");
        assert_eq!(&answer.body, bytes, "{path}");
        assert_eq!(header(&answer, header::CACHE_CONTROL), "public, max-age=31536000, immutable");
        assert_eq!(header(&answer, header::X_CONTENT_TYPE_OPTIONS), "nosniff");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn missing_files_and_api_paths_are_not_pages(pool: PgPool) {
    let app = app(pool);
    assert_eq!(send(&app, Method::GET, "/assets/missing-1234.js").await.status, StatusCode::NOT_FOUND);
    for path in ["/api/admin/nothing", "/api/v1/nothing", "/api", "/health/secrets"] {
        let answer = send(&app, Method::GET, path).await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{path}");
        let body: serde_json::Value = serde_json::from_slice(&answer.body).unwrap();
        assert_eq!(body["error"], "not_found", "{path}");
    }
    assert_eq!(send(&app, Method::POST, "/licences").await.status, StatusCode::METHOD_NOT_ALLOWED);
}

/// Regression: the 0.4.0 release left the dashboard's `package.json` at 0.3.0. The dashboard and
/// its lockfile carry the server's version.
#[test]
fn the_dashboard_carries_the_server_version() {
    let read = |file: &str| -> serde_json::Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/admin/").to_owned() + file;
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    };
    let (package, lock) = (read("package.json"), read("package-lock.json"));
    assert_eq!(package["version"], env!("CARGO_PKG_VERSION"), "package.json");
    assert_eq!(lock["version"], env!("CARGO_PKG_VERSION"), "package-lock.json");
    assert_eq!(lock["packages"][""]["version"], env!("CARGO_PKG_VERSION"), "package-lock.json root package");
}
