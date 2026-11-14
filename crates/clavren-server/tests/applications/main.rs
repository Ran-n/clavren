// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:49.249876
//+ Revised: 	2026/11/14 12:05:25.421701
// -----------------------------------------------------------------------------

//! Licence applications from the applicant's side and the administrator's: what the form offers,
//! sending one, asking how it went with the secret, approving (which issues the licence whose key
//! the applicant then collects and activates) and rejecting with a reason.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

#[path = "../common/mod.rs"]
mod common;
mod deciding;
mod key_page;

use axum::http::{Method, StatusCode, header};
use common::applying::{Applying, fake_turnstile};
use common::capture_logs;
use common::device::Device;
use common::licensing::PRODUCT;
use serde_json::{Value, json};
use sqlx::PgPool;

async fn audit_actions(pool: &PgPool) -> Vec<(String, Value)> {
    sqlx::query_as("SELECT action, metadata FROM audit_logs WHERE entity_type = 'application' ORDER BY created_at, id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_form_offers_the_active_products_that_take_applications(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let admin = applying.admin();
    admin.create("/api/admin/products", json!({ "code": "closed", "name": "Closed", "test_licence": false })).await;
    admin
        .create(
            "/api/admin/products",
            json!({ "code": "paused", "name": "Paused", "is_active": false, "accepts_applications": true, "test_licence": false }),
        )
        .await;
    let form = common::api::Call::new(Method::GET, "/api/v1/applications/form").send(applying.licensing.app()).await;
    assert_eq!(form.status, StatusCode::OK);
    assert_eq!(form.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(form.body["products"], json!([{ "code": PRODUCT, "name": "Avalio Lite", "description": null }]));
    assert_eq!(form.body["turnstile_site_key"], Value::Null, "no human check without Turnstile keys");
    assert!(form.body["server_time"].is_string());
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_form_names_the_turnstile_site_key(pool: PgPool) {
    let applying = Applying::with_settings(pool, fake_turnstile().await).await;
    let form = common::api::Call::new(Method::GET, "/api/v1/applications/form").send(applying.licensing.app()).await;
    assert_eq!(form.body["turnstile_site_key"], "site-key");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_application_waits_for_the_administrator(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (logs, _guard) = capture_logs();
    let reply = applying.apply(json!({ "message": "For my class of 2027" })).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");
    let (id, secret) = (reply.body["application"].as_str().unwrap(), reply.body["secret"].as_str().unwrap());
    assert_eq!(clavren_protocol::encoding::decode_array::<32>(secret).map(|_| ()), Some(()), "32 bytes, base64url");

    let status = applying.status(secret).await;
    assert_eq!(status.status, StatusCode::OK, "{:?}", status.body);
    assert_eq!((&status.body["status"], &status.body["product"]), (&json!("pending"), &json!(PRODUCT)));
    assert_eq!((&status.body["key"], &status.body["reason"]), (&Value::Null, &Value::Null));

    let listed = applying.admin().get("/api/admin/applications?state=pending&expand=product").await.body;
    let application = &listed["data"][0];
    assert_eq!((application["id"].as_str(), &application["state"]), (Some(id), &json!("pending")));
    assert_eq!((&application["name"], &application["email"]), (&json!("Ana López"), &json!("ana@example.com")));
    assert_eq!((&application["source"], &application["product"]["code"]), (&json!("app"), &json!(PRODUCT)));
    assert_eq!(application["message"], "For my class of 2027");
    assert!(application.get("secret").is_none() && application.get("secret_hash").is_none());
    assert_eq!(applying.admin().get("/api/admin/overview").await.body["pending_applications"], 1);

    let audit = audit_actions(applying.admin().pool()).await;
    assert_eq!(audit.len(), 1);
    assert_eq!((audit[0].0.as_str(), &audit[0].1["source"]), ("application.created", &json!("app")));
    let stored = serde_json::to_string(&audit).unwrap();
    assert!(!stored.contains("Ana") && !stored.contains("ana@example.com"), "{stored}");
    let text = logs.text();
    assert!(text.contains("licence application received"), "{text}");
    for secret_or_personal in [secret, "Ana", "ana@example.com", "2027"] {
        assert!(!text.contains(secret_or_personal), "logged {secret_or_personal}: {text}");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn only_a_product_taking_applications_gets_them(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let admin = applying.admin();
    admin.create("/api/admin/products", json!({ "code": "closed", "name": "Closed", "test_licence": false })).await;
    for product in ["closed", "no-such-product"] {
        let reply = applying.apply(json!({ "product": product })).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::FORBIDDEN, "product_unavailable"), "{product}");
        assert!(reply.body["server_time"].is_string());
    }
    let path = format!("/api/admin/products/{}", applying.licensing.product_id);
    admin.call(Method::PATCH, &path, Some(json!({ "is_active": false }))).await;
    assert_eq!(applying.apply(json!({})).await.error(), "product_unavailable", "an inactive product takes none");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_application_needs_a_name_and_an_address(pool: PgPool) {
    let applying = Applying::new(pool).await;
    for extra in [
        json!({ "name": " " }),
        json!({ "email": "" }),
        json!({ "email": "not an address" }),
        json!({ "name": "x".repeat(201) }),
        json!({ "message": "x".repeat(2001) }),
        json!({ "source": "fax" }),
        json!({ "extra": true }),
    ] {
        let reply = applying.apply(extra.clone()).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{extra}");
        // every attempt counts against the address's five an hour
        applying.licensing.admin.fixture.clock.advance(std::time::Duration::from_secs(3600));
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn one_address_has_one_application_waiting_per_product(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (first, _) = applying.applied(json!({})).await;
    let again = applying.apply(json!({ "email": "ANA@example.com" })).await;
    assert_eq!((again.status, again.error()), (StatusCode::CONFLICT, "application_pending"));
    applying
        .admin()
        .create(
            "/api/admin/products",
            json!({ "code": "other", "name": "Other", "accepts_applications": true, "test_licence": false }),
        )
        .await;
    assert_eq!(applying.apply(json!({ "product": "other" })).await.status, StatusCode::CREATED, "another product");
    assert_eq!(applying.reject(&first, json!({})).await.status, StatusCode::OK);
    assert_eq!(applying.apply(json!({})).await.status, StatusCode::CREATED, "once decided, a new one may come");
}
