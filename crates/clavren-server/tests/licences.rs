// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:17.102897
//+ Revised: 	2026/11/12 23:57:45.981045
// -----------------------------------------------------------------------------

//! Issuing and finding licences: keys stored only as hash and ciphertext, customers chosen or typed
//! in place, the message, search by key, name or ID, filters and exclusions, orders, and `expand`.

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

/// Four licences: Ana's (avalio, expiring in 10 days), one of avalio expiring in 100 days, and two
/// permanent ones of `other` without a customer, the second of them revoked.
struct Mixed {
    avalio: String,
    other: String,
    ana: Value,
    permanent: Value,
    revoked_id: String,
}

impl Mixed {
    async fn new(admin: &Admin) -> Self {
        let (avalio, other) = (product(admin, "avalio-lite").await, product(admin, "other").await);
        let now = admin.fixture.clock.now();
        let soon = (now + chrono::Duration::days(10)).to_rfc3339();
        let later = (now + chrono::Duration::days(100)).to_rfc3339();
        let ana = issue(admin, json!({ "product": avalio, "customer": { "name": "Ana" }, "expires_at": soon })).await;
        issue(admin, json!({ "product": avalio, "expires_at": later })).await;
        let permanent = issue(admin, json!({ "product": other })).await;
        let revoked = issue(admin, json!({ "product": other })).await;
        let revoked_id = revoked["licences"][0]["id"].as_str().unwrap().to_string();
        admin.call(Method::POST, &format!("/api/admin/licences/{revoked_id}/revoke"), Some(json!({}))).await;
        Self { avalio, other, ana, permanent, revoked_id }
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn filters_narrow_by_product_customer_state_and_expiry(pool: PgPool) {
    let mut admin = Admin::new(pool).await;
    let Mixed { avalio, other, ana, permanent, revoked_id } = Mixed::new(&admin).await;

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
async fn exclusions_hide_what_they_name(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let mixed = Mixed::new(&admin).await;
    let ids = |licences: Vec<Value>| licences.iter().map(|l| l["id"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    let ana_id = mixed.ana["licences"][0]["id"].as_str().unwrap().to_string();
    let ana_key = mixed.ana["licences"][0]["key"].as_str().unwrap();

    let live = ids(list(&admin, "?not_state=revoked").await);
    assert_eq!(live.len(), 3);
    assert!(!live.contains(&mixed.revoked_id));
    let avalio = list(&admin, &format!("?not_product={}", mixed.other)).await;
    assert!(avalio.len() == 2 && avalio.iter().all(|l| l["product"] == mixed.avalio.as_str()), "{avalio:?}");
    assert_eq!(list(&admin, "?not_expiry=permanent").await.len(), 2);
    let without_ana =
        ids(list(&admin, &format!("?not_customer={}", mixed.ana["customer"]["id"].as_str().unwrap())).await);
    assert_eq!(without_ana.len(), 3, "licences without a customer aren't hidden");
    assert!(!without_ana.contains(&ana_id));
    assert_eq!(list(&admin, "?not_q=ana").await.len(), 3);
    for hide in [ana_key, ana_id.as_str()] {
        let shown = ids(list(&admin, &format!("?not_q={}", urlencode(hide))).await);
        assert_eq!(shown.len(), 3, "{hide}");
        assert!(!shown.contains(&ana_id), "{hide}");
    }
    assert_eq!(list(&admin, &format!("?state=unused&not_product={}", mixed.other)).await.len(), 2);
    assert_eq!(list(&admin, "?not_q=%20%20").await.len(), 4, "blank hides nothing");
    assert_eq!(list(&admin, "?state=revoked&not_state=revoked").await.len(), 0, "consistent, just empty");
    assert_eq!(list(&admin, &format!("?q=ana&not_q={}", urlencode(ana_key))).await.len(), 0);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn bad_exclusions_are_refused(pool: PgPool) {
    let admin = Admin::new(pool).await;
    for bad in ["?not_state=lost", "?not_expiry=soon", "?not_product=nope", "?not_customer=nope"] {
        let reply = admin.get(&format!("/api/admin/licences{bad}")).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{bad}");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn export_honours_exclusions(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let mixed = Mixed::new(&admin).await;
    let body = json!({ "filter": { "not_state": "revoked" } });
    let reply = admin.call(Method::POST, "/api/admin/licences/export", Some(body)).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    let rows: Vec<&str> = reply.text.split("\r\n").skip(1).filter(|line| !line.is_empty()).collect();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| !row.contains(",revoked,") && !row.starts_with(&mixed.revoked_id)), "{rows:?}");
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
    let revoked = issue(&admin, json!({ "product": product })).await["licences"][0]["id"].as_str().unwrap().to_string();
    admin.call(Method::POST, &format!("/api/admin/licences/{revoked}/revoke"), Some(json!({}))).await;

    let walk = |query: &'static str| {
        let admin = &admin;
        async move {
            let mut seen = Vec::new();
            let mut page = format!("?{query}&limit=2");
            loop {
                let reply = admin.get(&format!("/api/admin/licences{page}")).await;
                assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
                seen.extend(reply.body["data"].as_array().unwrap().iter().cloned());
                let Some(cursor) = reply.body["next_cursor"].as_str() else { return seen };
                page = format!("?{query}&limit=2&after={cursor}");
            }
        }
    };
    let days = |licences: &[Value]| -> Vec<Option<i64>> {
        licences
            .iter()
            .map(|licence| {
                licence["expires_at"]
                    .as_str()
                    .map(|at| (chrono::DateTime::parse_from_rfc3339(at).unwrap().to_utc() - now).num_days())
            })
            .collect()
    };
    assert_eq!(days(&walk("order=expiry&not_state=revoked").await), [Some(10), Some(20), Some(30), None, None]);
    assert_eq!(walk("order=expiry").await.len(), 6);
    assert_eq!(walk("order=newest").await.len(), 6);
    let live = walk("order=newest&not_state=revoked").await;
    let distinct: std::collections::HashSet<&str> = live.iter().map(|l| l["id"].as_str().unwrap()).collect();
    assert_eq!((live.len(), distinct.len()), (5, 5), "no repeats");
    assert!(!distinct.contains(revoked.as_str()));
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
            "active_computers",
            "created_at",
            "customer",
            "expires_at",
            "id",
            "internal_notes",
            "is_test",
            "price",
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

async fn priced_product(admin: &Admin, code: &str, amount: i64) -> String {
    let id = product(admin, code).await;
    let path = format!("/api/admin/products/{id}");
    let reply =
        admin.call(Method::PATCH, &path, Some(json!({ "price": { "amount": amount, "currency": "EUR" } }))).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    id
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn issuing_takes_the_products_price_by_default(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = priced_product(&admin, "avalio-lite", 1999).await;
    let issued = issue(&admin, json!({ "product": product, "quantity": 2 })).await;
    for licence in issued["licences"].as_array().unwrap() {
        assert_eq!(licence["price"], json!({ "amount": 1999, "currency": "EUR" }));
    }
    let test = issue(&admin, json!({ "product": product, "test": true })).await;
    assert_eq!(test["licences"][0]["price"], Value::Null, "a test licence is never a sale");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn issuing_with_another_price_or_none(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let priced = priced_product(&admin, "avalio-lite", 1999).await;
    let other = json!({ "amount": 1500, "currency": "USD" });
    let issued = issue(&admin, json!({ "product": priced, "price": other })).await;
    assert_eq!(issued["licences"][0]["price"], other);
    let none = issue(&admin, json!({ "product": priced, "price": null })).await;
    assert_eq!(none["licences"][0]["price"], Value::Null);
    let unpriced = product(&admin, "free").await;
    assert_eq!(issue(&admin, json!({ "product": unpriced })).await["licences"][0]["price"], Value::Null);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_test_licence_with_a_price_is_refused(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let body = json!({ "product": product, "test": true, "price": { "amount": 1, "currency": "EUR" } });
    let reply = admin.call(Method::POST, "/api/admin/licences", Some(body)).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"));
    assert!(reply.body["message"].as_str().unwrap().contains("test licences can't have a price"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_licence_price_is_edited_in_place_and_in_bulk(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let id = issue(&admin, json!({ "product": product })).await["licences"][0]["id"].as_str().unwrap().to_string();
    let test = issue(&admin, json!({ "product": product, "test": true })).await["licences"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let price = json!({ "amount": 2500, "currency": "EUR" });
    let edited = admin.call(Method::PATCH, &format!("/api/admin/licences/{id}"), Some(json!({ "price": price }))).await;
    assert_eq!((edited.status, &edited.body["price"]), (StatusCode::OK, &price));
    let refused =
        admin.call(Method::PATCH, &format!("/api/admin/licences/{test}"), Some(json!({ "price": price }))).await;
    assert_eq!((refused.status, refused.error()), (StatusCode::CONFLICT, "test_licence_price"));

    let free = json!({ "amount": 0, "currency": "EUR" });
    let bulk = admin
        .call(Method::POST, "/api/admin/licences/bulk/edit", Some(json!({ "ids": [id, test], "price": free })))
        .await;
    assert_eq!(bulk.status, StatusCode::OK, "{:?}", bulk.body);
    assert_eq!((&bulk.body["updated"], &bulk.body["kept_test"]), (&json!(1), &json!(1)));
    assert_eq!(admin.get(&format!("/api/admin/licences/{id}")).await.body["price"], free);
    assert_eq!(admin.get(&format!("/api/admin/licences/{test}")).await.body["price"], Value::Null);
    let fields: Vec<Value> = sqlx::query_scalar(
        "SELECT metadata FROM audit_logs WHERE action = 'licence.updated' AND entity_id = $1 ORDER BY created_at, id",
    )
    .bind(Uuid::parse_str(&id).unwrap())
    .fetch_all(admin.pool())
    .await
    .unwrap();
    assert_eq!(fields, vec![json!({ "fields": ["price"] }); 2]);
    let cleared = admin.call(Method::PATCH, &format!("/api/admin/licences/{id}"), Some(json!({ "price": null }))).await;
    assert_eq!(cleared.body["price"], Value::Null);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_csv_ends_with_price_and_currency(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = priced_product(&admin, "avalio-lite", 1999).await;
    let priced = issue(&admin, json!({ "product": product })).await["licences"][0]["id"].clone();
    let unpriced = issue(&admin, json!({ "product": product, "price": null })).await["licences"][0]["id"].clone();
    let reply =
        admin.call(Method::POST, "/api/admin/licences/export", Some(json!({ "ids": [priced, unpriced] }))).await;
    let lines: Vec<&str> = reply.text.trim_end().split("\r\n").collect();
    assert!(lines[0].ends_with(",test,price,currency"), "{}", lines[0]);
    let priced_line = lines.iter().find(|line| line.starts_with(priced.as_str().unwrap())).unwrap();
    assert!(priced_line.ends_with(",false,19.99,EUR"), "{priced_line}");
    let unpriced_line = lines.iter().find(|line| line.starts_with(unpriced.as_str().unwrap())).unwrap();
    assert!(unpriced_line.ends_with(",false,,"), "{unpriced_line}");
}
