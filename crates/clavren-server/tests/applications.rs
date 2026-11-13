// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:49.249876
//+ Revised: 	2026/11/12 23:57:45.730658
// -----------------------------------------------------------------------------

//! Licence applications from the applicant's side and the administrator's: what the form offers,
//! sending one, asking how it went with the secret, approving (which issues the licence whose key
//! the applicant then collects and activates) and rejecting with a reason.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

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

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn approving_issues_the_licence_the_applicant_collects(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (id, secret) = applying.applied(json!({})).await;
    let expires = (applying.licensing.now() + chrono::Duration::days(300)).to_rfc3339();
    let body = json!({
        "customer": { "name": "Ana López", "email": "ana@example.com" },
        "expires_at": expires,
        "internal_notes": "applied online",
    });
    let approved = applying.approve(&id, body).await;
    assert_eq!(approved.status, StatusCode::OK, "{:?}", approved.body);
    assert_eq!(approved.body["state"], "approved");
    let licence = &approved.body["licence"];
    assert_eq!((&licence["customer"]["email"], &licence["is_test"]), (&json!("ana@example.com"), &json!(false)));
    assert_eq!(licence["internal_notes"], "applied online");
    assert_eq!(approved.body["key_collected_at"], Value::Null);

    let status = applying.status(&secret).await;
    assert_eq!(status.body["status"], "approved");
    let key = status.body["key"].as_str().unwrap();
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(status.body["licence_expires_at"].as_str().unwrap()).unwrap().to_rfc3339(),
        expires
    );
    let device = Device::new(7);
    let activated = device.activate(applying.licensing.app(), PRODUCT, key).await;
    assert_eq!(activated.status, StatusCode::CREATED, "the key works: {:?}", activated.body);
    let revealed = applying
        .licensing
        .admin_post(&format!("/api/admin/licences/{}/reveal", licence["id"].as_str().unwrap()), json!({}))
        .await;
    assert_eq!(revealed.body["key"], key);

    assert_eq!(applying.status(&secret).await.body["key"], key, "the key can be collected again");
    assert!(applying.application(&id).await["key_collected_at"].is_string());
    let actions: Vec<String> =
        audit_actions(applying.admin().pool()).await.into_iter().map(|(action, _)| action).collect();
    assert_eq!(actions, ["application.created", "application.approved", "application.key_collected"], "collected once");
    assert_eq!(applying.admin().get("/api/admin/overview").await.body["pending_applications"], 0);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_application_is_approved_for_an_existing_customer_or_none(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let customer =
        applying.admin().create("/api/admin/customers", json!({ "name": "Ana", "email": "ana@example.com" })).await;
    let (with_customer, _) = applying.applied(json!({})).await;
    let approved = applying.approve(&with_customer, json!({ "customer": customer["id"] })).await;
    assert_eq!(approved.body["licence"]["customer"]["id"], customer["id"]);
    let clash = applying.applied(json!({})).await.0;
    let taken = applying.approve(&clash, json!({ "customer": { "name": "Ana", "email": "Ana@Example.com" } })).await;
    assert_eq!((taken.status, taken.error()), (StatusCode::CONFLICT, "customer_email_taken"));
    assert_eq!(applying.application(&clash).await["state"], "pending", "nothing was issued");
    let without = applying.approve(&clash, json!({})).await;
    assert_eq!((without.status, &without.body["licence"]["customer"]), (StatusCode::OK, &Value::Null));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_application_is_decided_once(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (approved, _) = applying.applied(json!({})).await;
    applying.approve(&approved, json!({})).await;
    for reply in [applying.approve(&approved, json!({})).await, applying.reject(&approved, json!({})).await] {
        assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "application_decided"));
    }
    let licences: i64 = sqlx::query_scalar("SELECT count(*) FROM licenses WHERE NOT is_test")
        .fetch_one(applying.admin().pool())
        .await
        .unwrap();
    assert_eq!(licences, 1);
    let missing = uuid::Uuid::now_v7();
    assert_eq!(applying.approve(&missing.to_string(), json!({})).await.status, StatusCode::NOT_FOUND);
    assert_eq!(applying.reject(&missing.to_string(), json!({})).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_rejection_tells_the_applicant_why(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (id, secret) = applying.applied(json!({})).await;
    let rejected = applying.reject(&id, json!({ "reason": " Only for schools " })).await;
    assert_eq!((rejected.status, &rejected.body["state"]), (StatusCode::OK, &json!("rejected")));
    assert_eq!(rejected.body["rejection_reason"], "Only for schools");
    let status = applying.status(&secret).await.body;
    assert_eq!(
        (&status["status"], &status["reason"], &status["key"]),
        (&json!("rejected"), &json!("Only for schools"), &Value::Null)
    );
    assert!(status["decided_at"].is_string());
    let too_long = applying.applied(json!({ "email": "bo@example.com" })).await.0;
    assert_eq!(applying.reject(&too_long, json!({ "reason": "x".repeat(501) })).await.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_revoked_or_deleted_licence_is_withdrawn(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (revoked, revoked_secret) = applying.applied(json!({})).await;
    let licence = applying.approve(&revoked, json!({})).await.body["licence"]["id"].as_str().unwrap().to_string();
    applying.licensing.admin_post(&format!("/api/admin/licences/{licence}/revoke"), json!({})).await;
    let status = applying.status(&revoked_secret).await.body;
    assert_eq!((&status["status"], &status["key"]), (&json!("withdrawn"), &Value::Null));

    let (deleted, deleted_secret) = applying.applied(json!({ "email": "bo@example.com" })).await;
    let licence = applying.approve(&deleted, json!({})).await.body["licence"]["id"].as_str().unwrap().to_string();
    applying.admin().call(Method::DELETE, &format!("/api/admin/licences/{licence}"), None).await;
    assert_eq!(applying.status(&deleted_secret).await.body["status"], "withdrawn");
    let application = applying.application(&deleted).await;
    assert_eq!((&application["state"], &application["licence"]), (&json!("approved"), &Value::Null));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_wrong_secret_names_no_application(pool: PgPool) {
    let applying = Applying::new(pool).await;
    applying.applied(json!({})).await;
    let random = clavren_protocol::encoding::encode(&[7; 32]);
    for secret in [random.as_str(), "short", "", "!!!"] {
        let reply = applying.status(secret).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::NOT_FOUND, "application_unknown"), "{secret:?}");
        assert!(reply.body["server_time"].is_string());
    }
}

async fn key_page(applying: &Applying, code: &str) -> common::api::Reply {
    common::api::Call::new(Method::GET, &format!("/api/v1/applications/products/{code}"))
        .send(applying.licensing.app())
        .await
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_key_page_says_how_to_get_a_key_for_an_active_product(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let admin = applying.admin();
    let path = format!("/api/admin/products/{}", applying.licensing.product_id);
    let text = "Buy one at https://example.com/shop";
    admin.call(Method::PATCH, &path, Some(json!({ "key_instructions": text, "description": "Marks" }))).await;

    let open = key_page(&applying, PRODUCT).await;
    assert_eq!(open.status, StatusCode::OK, "{:?}", open.body);
    assert_eq!(open.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!((&open.body["code"], &open.body["name"]), (&json!(PRODUCT), &json!("Avalio Lite")));
    assert_eq!((&open.body["description"], &open.body["key_instructions"]), (&json!("Marks"), &json!(text)));
    assert_eq!(open.body["accepts_applications"], true);
    assert!(open.body["server_time"].is_string());

    admin.call(Method::PATCH, &path, Some(json!({ "accepts_applications": false }))).await;
    let closed = key_page(&applying, PRODUCT).await.body;
    assert_eq!((&closed["accepts_applications"], &closed["key_instructions"]), (&json!(false), &json!(text)));

    admin.call(Method::PATCH, &path, Some(json!({ "key_instructions": null }))).await;
    let bare = key_page(&applying, PRODUCT).await.body;
    assert_eq!((&bare["accepts_applications"], &bare["key_instructions"]), (&json!(false), &Value::Null));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_inactive_or_unknown_product_has_no_key_page(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let path = format!("/api/admin/products/{}", applying.licensing.product_id);
    applying.admin().call(Method::PATCH, &path, Some(json!({ "is_active": false }))).await;
    let (logs, _guard) = capture_logs();
    let mut bodies = Vec::new();
    for code in [PRODUCT, "no-such-product"] {
        let reply = key_page(&applying, code).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{code}");
        assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");
        assert!(reply.body["server_time"].is_string());
        let mut body = reply.body.clone();
        body.as_object_mut().unwrap().remove("server_time");
        bodies.push(body);
    }
    assert_eq!(bodies[0], bodies[1], "inactive and unknown read the same");
    assert_eq!(bodies[0]["error"], "product_unavailable");
    assert!(logs.text().contains("key page refused"), "{}", logs.text());
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn approving_records_no_price_unless_given(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let path = format!("/api/admin/products/{}", applying.licensing.product_id);
    let price = json!({ "amount": 1999, "currency": "EUR" });
    applying.admin().call(Method::PATCH, &path, Some(json!({ "price": price }))).await;
    let (plain, _) = applying.applied(json!({})).await;
    let approved = applying.approve(&plain, json!({})).await;
    assert_eq!(approved.body["licence"]["price"], Value::Null, "an application is usually not a sale");
    let (sold, _) = applying.applied(json!({ "email": "bea@example.com" })).await;
    let approved = applying.approve(&sold, json!({ "price": price })).await;
    assert_eq!(approved.body["licence"]["price"], price);
}
