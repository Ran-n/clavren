// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:17.102897
//+ Revised: 	2026/11/14 12:05:26.842846
// -----------------------------------------------------------------------------

//! Issuing and finding licences: keys stored only as hash and ciphertext, customers chosen or typed
//! in place, the message, search by key, name or ID, filters and exclusions, orders, and `expand`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

#[path = "../common/mod.rs"]
mod common;
mod filters;
mod paging;
mod prices;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use clavren::clock::Clock;
use clavren_protocol::licence_key::LicenceKey;
use common::api::Admin;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn product(admin: &Admin, code: &str) -> String {
    let body = json!({ "code": code, "name": format!("Product {code}"), "message_template": "Hi {customer_name}, your {product_name} key: {key}", "test_licence": false });
    admin.create("/api/admin/products", body).await["id"].as_str().unwrap().to_string()
}

async fn issue(admin: &Admin, body: Value) -> Value {
    admin.create("/api/admin/licences", body).await
}

async fn list(admin: &Admin, query: &str) -> Vec<Value> {
    let reply = admin.get(&format!("/api/admin/licences{query}")).await;
    assert_eq!(reply.status, StatusCode::OK, "{query}: {:?}", reply.body);
    reply.body["data"].as_array().unwrap().clone()
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn issued_keys_are_shown_once_and_stored_protected(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let issued = issue(&admin, json!({ "product": product, "quantity": 3, "internal_notes": "School order" })).await;

    let licences = issued["licences"].as_array().unwrap();
    assert_eq!(licences.len(), 3);
    assert_eq!((issued["customer"].clone(), issued["message"].clone()), (Value::Null, Value::Null));
    let first = &licences[0];
    assert_eq!(
        (first["product"].as_str(), first["state"].as_str(), first["customer"].clone()),
        (Some(product.as_str()), Some("unused"), Value::Null)
    );
    assert_eq!(first["internal_notes"], "School order");
    let keys: Vec<LicenceKey> =
        licences.iter().map(|licence| LicenceKey::parse(licence["key"].as_str().unwrap()).unwrap()).collect();
    assert!(licences[0]["key"].as_str().unwrap().starts_with("BRN-"));
    assert_ne!(keys[0], keys[1]);

    let stored: String = sqlx::query_scalar(
        "SELECT concat_ws(' ', (SELECT string_agg(l::text, ' ') FROM licenses l), (SELECT string_agg(k::text, ' ') FROM license_key_secrets k),
                (SELECT string_agg(a::text, ' ') FROM audit_logs a))",
    )
    .fetch_one(admin.pool())
    .await
    .unwrap();
    for key in &keys {
        assert!(!stored.contains(&key.canonical()[3..]), "a key is stored in plain");
    }
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = 'licence.issued'")
        .fetch_one(admin.pool())
        .await
        .unwrap();
    assert_eq!(audited, 3, "one entry per licence");
    let listed = list(&admin, "").await;
    assert!(listed.iter().all(|licence| licence.get("key").is_none()), "lists never show keys");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_customer_typed_in_place_is_created_and_gets_a_message(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let body =
        json!({ "product": product, "quantity": 2, "customer": { "name": "Ana López", "email": "ana@example.com" } });
    let issued = issue(&admin, body).await;

    let customer = &issued["customer"];
    assert_eq!(customer["name"], "Ana López");
    let message = issued["message"].as_str().unwrap();
    let keys: Vec<&str> =
        issued["licences"].as_array().unwrap().iter().map(|licence| licence["key"].as_str().unwrap()).collect();
    assert_eq!(message, format!("Hi Ana López, your Product avalio-lite key: {}\n{}", keys[0], keys[1]));
    assert!(issued["licences"].as_array().unwrap().iter().all(|licence| licence["customer"] == customer["id"]));

    let again = issue(&admin, json!({ "product": product, "customer": customer["id"] })).await;
    assert_eq!(again["customer"]["id"], customer["id"], "an existing customer by ID");
    let reply = admin
        .call(
            Method::POST,
            "/api/admin/licences",
            Some(json!({ "product": product, "customer": { "name": "Other", "email": "ANA@example.com" } })),
        )
        .await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "customer_email_taken"));
    let licences: i64 = sqlx::query_scalar("SELECT count(*) FROM licenses").fetch_one(admin.pool()).await.unwrap();
    assert_eq!(licences, 3, "the refused request issued nothing");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn issuing_checks_its_request(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let past = (admin.fixture.clock.now() - chrono::Duration::days(400)).to_rfc3339();
    for body in [
        json!({ "product": product, "quantity": 0 }),
        json!({ "product": product, "quantity": 1001 }),
        json!({ "product": product, "expires_at": past }),
        json!({ "product": Uuid::now_v7() }),
        json!({ "product": product, "customer": Uuid::now_v7() }),
        json!({ "product": product, "customer": { "name": "" } }),
        json!({ "product": product, "internal_notes": "x".repeat(2001) }),
        json!({ "product": product, "colour": "red" }),
    ] {
        let reply = admin.call(Method::POST, "/api/admin/licences", Some(body.clone())).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{body}");
    }
    admin.call(Method::PATCH, &format!("/api/admin/products/{product}"), Some(json!({ "is_active": false }))).await;
    let reply = admin.call(Method::POST, "/api/admin/licences", Some(json!({ "product": product }))).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "product_inactive"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_thousand_licences_are_issued_at_once(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let issued = issue(&admin, json!({ "product": product, "quantity": 1000 })).await;
    let keys: std::collections::HashSet<&str> =
        issued["licences"].as_array().unwrap().iter().map(|licence| licence["key"].as_str().unwrap()).collect();
    assert_eq!(keys.len(), 1000);
}

fn urlencode(text: &str) -> String {
    form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

/// Issues `quantity` licences whose keys are made from `bytes` in turn (then from 7s).
async fn issue_with_key_bytes(
    admin: &Admin,
    product: &Value,
    quantity: usize,
    bytes: &[u8],
) -> Result<Vec<LicenceKey>, clavren::http::error::ApiError> {
    use clavren::audit::Actor;
    use clavren::licences::{IssueRequest, issue_with_keys};

    let actor =
        Actor { admin_id: admin.fixture.admin.id, ip: [198, 51, 100, 10].into(), at: admin.fixture.clock.now() };
    let request: IssueRequest =
        serde_json::from_value(json!({ "product": product["id"], "quantity": quantity })).unwrap();
    let mut bytes = bytes.iter().copied();
    let mut generate = move || Ok(LicenceKey::from_random([bytes.next().unwrap_or(7); 10]));
    let mut transaction = admin.pool().begin().await.unwrap();
    let issued = issue_with_keys(&admin.fixture.state, &mut transaction, &actor, request, &mut generate).await?;
    transaction.commit().await.unwrap();
    Ok(issued.keys.into_iter().map(|(_, key)| key).collect())
}

/// A generated key that already exists (80 random bits: it never happens by itself) is made
/// again, and only that one; keys that keep colliding stop the batch.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_key_that_already_exists_is_generated_again(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = admin.create("/api/admin/products", json!({ "name": "Avalio", "test_licence": false })).await;
    let key = |byte: u8| LicenceKey::from_random([byte; 10]);

    assert_eq!(issue_with_key_bytes(&admin, &product, 1, &[7]).await.unwrap(), [key(7)]);
    let (logs, _guard) = common::capture_logs();
    let mut second = issue_with_key_bytes(&admin, &product, 2, &[7, 8, 9]).await.unwrap();
    second.sort_by_key(|key| key.canonical().to_string());
    assert_eq!(second, [key(8), key(9)], "only the key that existed was made again");
    assert!(logs.text().contains("collided"), "{}", logs.text());
    let error = issue_with_key_bytes(&admin, &product, 1, &[7, 8, 9]).await.unwrap_err();
    assert_eq!(error.code(), "internal_error");
    let listed = admin.call(Method::GET, "/api/admin/licences", None).await;
    assert_eq!(listed.body["data"].as_array().unwrap().len(), 3, "the batch that kept colliding left nothing");
    for byte in [7, 8, 9] {
        let found = admin.call(Method::GET, &format!("/api/admin/licences?q={}", key(byte).formatted()), None).await;
        assert_eq!(found.body["data"].as_array().unwrap().len(), 1, "key {byte} opens one licence");
    }
}
