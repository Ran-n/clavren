// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:17.102897
//+ Revised: 	2026/11/03 04:54:17.102897
// -----------------------------------------------------------------------------

//! Issuing and finding licences: keys stored only as hash and ciphertext, customers chosen or typed
//! in place, the message, search by key, name or ID, filters, orders, and `expand`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use clavren::clock::Clock;
use clavren_protocol::licence_key::LicenceKey;
use common::api::Admin;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn product(admin: &Admin, code: &str) -> String {
    let body = json!({ "code": code, "name": format!("Product {code}"), "message_template": "Hi {customer_name}, your {product_name} key: {key}" });
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

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn search_finds_a_key_however_it_is_typed_and_names_and_ids(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let ana = issue(
        &admin,
        json!({ "product": product, "customer": { "name": "Ana López", "email": "ana@school.example" } }),
    )
    .await;
    issue(&admin, json!({ "product": product, "customer": { "name": "Bruno" } })).await;
    let ana_licence = &ana["licences"][0];
    let key = ana_licence["key"].as_str().unwrap();

    let sloppy = key.to_lowercase().replace('-', " ").replace('0', "o");
    for query in
        [key.to_string(), sloppy, "lópez".into(), "SCHOOL".into(), ana_licence["id"].as_str().unwrap().to_string()]
    {
        let found = list(&admin, &format!("?q={}", urlencode(&query))).await;
        assert_eq!(found.len(), 1, "{query}");
        assert_eq!(found[0]["id"], ana_licence["id"], "{query}");
    }
    assert!(list(&admin, "?q=BRN-0000-0000-0000-0000").await.is_empty());
    assert_eq!(list(&admin, "?q=%20").await.len(), 2);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn filters_narrow_by_product_customer_state_and_expiry(pool: PgPool) {
    let mut admin = Admin::new(pool).await;
    let (avalio, other) = (product(&admin, "avalio-lite").await, product(&admin, "other").await);
    let now = admin.fixture.clock.now();
    let soon = (now + chrono::Duration::days(10)).to_rfc3339();
    let later = (now + chrono::Duration::days(100)).to_rfc3339();
    let ana = issue(&admin, json!({ "product": avalio, "customer": { "name": "Ana" }, "expires_at": soon })).await;
    issue(&admin, json!({ "product": avalio, "expires_at": later })).await;
    let permanent = issue(&admin, json!({ "product": other })).await;
    let revoked = issue(&admin, json!({ "product": other })).await;
    let revoked_id = revoked["licences"][0]["id"].as_str().unwrap();
    admin.call(Method::POST, &format!("/api/admin/licences/{revoked_id}/revoke"), Some(json!({}))).await;

    assert_eq!(list(&admin, &format!("?product={avalio}")).await.len(), 2);
    assert_eq!(list(&admin, &format!("?customer={}", ana["customer"]["id"].as_str().unwrap())).await.len(), 1);
    assert_eq!(list(&admin, "?state=revoked").await[0]["id"], revoked_id);
    assert_eq!(list(&admin, "?state=unused").await.len(), 3);
    assert_eq!(list(&admin, "?expiry=expiring").await[0]["id"], ana["licences"][0]["id"]);
    assert_eq!(list(&admin, "?expiry=permanent").await.len(), 2);
    let permanent_id = &permanent["licences"][0]["id"];
    assert_eq!(list(&admin, &format!("?expiry=permanent&state=unused&product={other}")).await[0]["id"], *permanent_id);

    // a day after the first one expired
    admin.wait(Duration::from_secs(11 * 24 * 3600)).await;
    let expired = list(&admin, "?expiry=expired").await;
    assert_eq!((expired.len(), &expired[0]["id"]), (1, &ana["licences"][0]["id"]));
    for bad in ["?state=lost", "?expiry=soon", "?product=nope", "?order=random", "?expand=owner", "?colour=red"] {
        let reply = admin.get(&format!("/api/admin/licences{bad}")).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{bad}");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn pages_follow_either_order_to_the_end(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let now = admin.fixture.clock.now();
    for days in [30, 10, 20] {
        issue(&admin, json!({ "product": product, "expires_at": (now + chrono::Duration::days(days)).to_rfc3339() }))
            .await;
    }
    issue(&admin, json!({ "product": product, "quantity": 2 })).await;

    let walk = |order: &'static str| {
        let admin = &admin;
        async move {
            let mut seen = Vec::new();
            let mut query = format!("?order={order}&limit=2");
            loop {
                let reply = admin.get(&format!("/api/admin/licences{query}")).await;
                assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
                seen.extend(reply.body["data"].as_array().unwrap().iter().map(|licence| licence["expires_at"].clone()));
                let Some(cursor) = reply.body["next_cursor"].as_str() else { return seen };
                query = format!("?order={order}&limit=2&after={cursor}");
            }
        }
    };
    let by_expiry = walk("expiry").await;
    let days: Vec<Option<i64>> = by_expiry
        .iter()
        .map(|at| at.as_str().map(|at| (chrono::DateTime::parse_from_rfc3339(at).unwrap().to_utc() - now).num_days()))
        .collect();
    assert_eq!(days, [Some(10), Some(20), Some(30), None, None]);
    assert_eq!(walk("newest").await.len(), 5);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn related_records_expand_on_request(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let issued = issue(&admin, json!({ "product": product, "customer": { "name": "Ana" } })).await;
    let id = issued["licences"][0]["id"].as_str().unwrap();

    let plain = admin.get(&format!("/api/admin/licences/{id}")).await.body;
    assert_eq!((plain["product"].as_str(), plain["active_activation"].clone()), (Some(product.as_str()), Value::Null));
    let keys: Vec<&str> = plain.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "active_activation",
            "created_at",
            "customer",
            "expires_at",
            "id",
            "internal_notes",
            "product",
            "revocation_reason",
            "revoked_at",
            "state"
        ]
    );

    let expanded = admin.get(&format!("/api/admin/licences/{id}?expand=product&expand=customer")).await.body;
    assert_eq!(expanded["product"]["code"], "avalio-lite");
    assert_eq!(expanded["customer"]["name"], "Ana");
    let listed = list(&admin, "?expand=product,customer,active_activation").await;
    assert_eq!(listed[0]["product"]["id"], product.as_str());
    assert_eq!(admin.get(&format!("/api/admin/licences/{}", Uuid::now_v7())).await.status, StatusCode::NOT_FOUND);
}

fn urlencode(text: &str) -> String {
    form_urlencoded::byte_serialize(text.as_bytes()).collect()
}
