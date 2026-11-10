// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/10 17:59:23.290454
//+ Revised: 	2026/11/10 17:59:23.368727
// -----------------------------------------------------------------------------

//! Test licences: one comes with every new product, more are issued like any licence; they work on
//! any number of computers, their tokens say so, ending one computer leaves the others, and the
//! Overview leaves them, their computers and their checks out.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use axum::http::{Method, StatusCode, header};
use clavren_protocol::licence_key::LicenceKey;
use common::api::Admin;
use common::device::{Device, verified};
use common::licensing::{Licensing, PRODUCT};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Issues one test licence of the [`Licensing`] product; returns its ID and key.
async fn test_licence(licensing: &Licensing) -> (String, String) {
    licensing.issue(json!({ "test": true })).await
}

/// Activates `key` on `device`, expecting a new activation; returns its ID.
async fn activate(licensing: &Licensing, device: &Device, key: &str) -> String {
    let reply = device.activate(licensing.app(), PRODUCT, key).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    reply.body["activation"].as_str().unwrap().to_string()
}

/// The IDs of the licences listed at `/api/admin/licences{query}`.
async fn listed(admin: &Admin, query: &str) -> Vec<String> {
    let page = admin.get(&format!("/api/admin/licences{query}")).await;
    assert_eq!(page.status, StatusCode::OK, "{:?}", page.body);
    page.body["data"].as_array().unwrap().iter().map(|licence| licence["id"].as_str().unwrap().to_string()).collect()
}

async fn overview(admin: &Admin) -> Value {
    admin.get("/api/admin/overview").await.body
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_new_product_comes_with_a_test_licence(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let reply = admin.call(Method::POST, "/api/admin/products", Some(json!({ "name": "Avalio Lite" }))).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store", "the answer carries a key");
    let product = reply.body["id"].as_str().unwrap();
    let created = &reply.body["test_licence"];
    let id = created["id"].as_str().unwrap();
    let key = created["key"].as_str().unwrap();
    assert_eq!(LicenceKey::parse(key).map(|parsed| parsed.formatted()).as_deref(), Some(key));

    let licence = admin.get(&format!("/api/admin/licences/{id}")).await.body;
    assert_eq!(
        (&licence["is_test"], licence["product"].as_str(), &licence["customer"], &licence["expires_at"]),
        (&json!(true), Some(product), &Value::Null, &Value::Null)
    );
    assert_eq!((&licence["state"], &licence["active_computers"]), (&json!("unused"), &json!(0)));
    let revealed = admin.call(Method::POST, &format!("/api/admin/licences/{id}/reveal"), Some(json!({}))).await;
    assert_eq!(revealed.body["key"], key, "stored like any key");

    let audit: Vec<(String, Value)> = sqlx::query_as(
        "SELECT action, metadata FROM audit_logs WHERE entity_type IN ('product', 'licence') ORDER BY created_at, id",
    )
    .fetch_all(admin.pool())
    .await
    .unwrap();
    let actions: Vec<&str> = audit.iter().map(|(action, _)| action.as_str()).collect();
    assert_eq!(actions, ["product.created", "licence.issued", "licence.key_revealed"]);
    assert_eq!(audit[1].1["test"], true);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_inactive_new_product_gets_one_too_and_none_when_asked(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let inactive = admin.create("/api/admin/products", json!({ "name": "Draft", "is_active": false })).await;
    assert!(inactive["test_licence"]["key"].is_string(), "testing comes before selling");
    let without = admin.create("/api/admin/products", json!({ "name": "Plain", "test_licence": false })).await;
    assert_eq!(without["test_licence"], Value::Null);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM licenses").fetch_one(admin.pool()).await.unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn test_licences_are_issued_listed_and_exported_as_such(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (test, _) = test_licence(&licensing).await;
    let (ordinary, _) = licensing.issue(json!({})).await;
    let only: Vec<String> = listed(&licensing.admin, "?test=true").await;
    assert_eq!(only, std::slice::from_ref(&test));
    let hidden: Vec<String> = listed(&licensing.admin, "?not_test=true").await;
    assert_eq!(hidden, std::slice::from_ref(&ordinary));
    let ordinary_only: Vec<String> = listed(&licensing.admin, "?test=false").await;
    assert_eq!(ordinary_only, std::slice::from_ref(&ordinary));
    assert_eq!(licensing.admin.get("/api/admin/licences?test=maybe").await.status, StatusCode::BAD_REQUEST);

    let csv = licensing.admin_post("/api/admin/licences/export", json!({ "filter": { "test": true } })).await;
    let lines: Vec<&str> = csv.text.split("\r\n").filter(|line| !line.is_empty()).collect();
    assert_eq!(lines.len(), 2, "{}", csv.text);
    assert!(lines[1].starts_with(&test) && lines[1].ends_with(",true"), "{}", lines[1]);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_test_licence_works_on_several_computers_and_its_tokens_say_so(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, key) = test_licence(&licensing).await;
    let now = licensing.now().timestamp();
    let mut activations = Vec::new();
    for seed in 1..=3 {
        let device = Device::new(seed);
        let reply = device.activate(licensing.app(), PRODUCT, &key).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
        assert!(verified(&reply, PRODUCT, now).test);
        activations.push(reply.body["activation"].as_str().unwrap().to_string());
    }
    let distinct: std::collections::HashSet<&String> = activations.iter().collect();
    assert_eq!(distinct.len(), 3, "one activation per computer");

    let again = Device::new(2).activate(licensing.app(), PRODUCT, &key).await;
    assert_eq!(again.status, StatusCode::OK, "the same computer retrying gets its activation back");
    assert_eq!(again.body["activation"], activations[1].as_str());

    let checked = Device::new(1).check(licensing.app(), &activations[0]).await;
    assert_eq!(checked.status, StatusCode::OK, "{:?}", checked.body);
    assert!(verified(&checked, PRODUCT, now).test);

    let computers = |query: &'static str| {
        let admin = &licensing.admin;
        async move { admin.get(&format!("/api/admin/activations{query}")).await.body["data"].as_array().unwrap().len() }
    };
    assert_eq!((computers("?test=true").await, computers("?test=false").await, computers("").await), (3, 0, 3));

    let view = licensing.licence(&licence).await;
    assert_eq!((&view["state"], &view["active_computers"]), (&json!("redeemed"), &json!(3)));
    assert_eq!(view["active_activation"]["id"], activations[2].as_str(), "the latest computer");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_ordinary_licence_stays_on_one_computer_and_its_tokens_say_nothing(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, key) = licensing.issue(json!({})).await;
    let first = Device::new(1).activate(licensing.app(), PRODUCT, &key).await;
    assert!(!verified(&first, PRODUCT, licensing.now().timestamp()).test);
    let claims = clavren_protocol::token::unverified_claims(first.body["token"].as_str().unwrap()).unwrap();
    assert!(!claims.test);
    let second = Device::new(2).activate(licensing.app(), PRODUCT, &key).await;
    assert_eq!((second.status, second.error()), (StatusCode::CONFLICT, "already_activated"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn ending_one_computer_leaves_the_others_and_a_reset_ends_them_all(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, key) = test_licence(&licensing).await;
    let first = activate(&licensing, &Device::new(1), &key).await;
    let second = activate(&licensing, &Device::new(2), &key).await;
    let third = activate(&licensing, &Device::new(3), &key).await;

    let ended = licensing.admin_post(&format!("/api/admin/activations/{first}/revoke"), json!({})).await;
    assert_eq!(ended.status, StatusCode::OK, "{:?}", ended.body);
    let refused = Device::new(1).challenge(licensing.app(), &first).await;
    assert_eq!(refused.error(), "activation_revoked");
    assert_eq!(Device::new(2).check(licensing.app(), &second).await.status, StatusCode::OK, "the others go on");
    assert_eq!(licensing.licence(&licence).await["active_computers"], 2);

    let reset = licensing.admin_post(&format!("/api/admin/licences/{licence}/reset"), json!({})).await;
    assert_eq!(reset.status, StatusCode::OK, "{:?}", reset.body);
    for (seed, activation) in [(2, &second), (3, &third)] {
        assert_eq!(Device::new(seed).challenge(licensing.app(), activation).await.error(), "activation_revoked");
    }
    let view = licensing.licence(&licence).await;
    assert_eq!((&view["state"], &view["active_computers"]), (&json!("unused"), &json!(0)));
    let metadata: Vec<Value> =
        sqlx::query_scalar("SELECT metadata FROM audit_logs WHERE action = 'licence.reset' ORDER BY created_at, id")
            .fetch_all(licensing.admin.pool())
            .await
            .unwrap();
    assert_eq!(metadata, [json!({ "activation": first }), json!({})]);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn revoking_a_test_licence_locks_every_computer(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, key) = test_licence(&licensing).await;
    let first = activate(&licensing, &Device::new(1), &key).await;
    let second = activate(&licensing, &Device::new(2), &key).await;
    // challenges asked for before the revocation, answered after it
    let mut pending = Vec::new();
    for (seed, activation) in [(1, &first), (2, &second)] {
        let device = Device::new(seed);
        let challenge = device.challenge(licensing.app(), activation).await;
        pending.push((device.check_body(activation, &challenge.body), device, activation));
    }
    licensing.admin_post(&format!("/api/admin/licences/{licence}/revoke"), json!({})).await;
    for (body, device, activation) in pending {
        assert_eq!(device.send_check(licensing.app(), activation, body).await.error(), "licence_revoked");
    }
    assert_eq!(
        Device::new(4).activate(licensing.app(), PRODUCT, &key).await.error(),
        "licence_revoked",
        "and no new one gets in"
    );
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_overview_leaves_test_licences_their_computers_and_checks_out(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, test_key) = test_licence(&licensing).await;
    let (_, key) = licensing.issue(json!({})).await;
    licensing.issue(json!({ "test": true })).await;
    let ordinary = activate(&licensing, &Device::new(1), &key).await;
    let tested = activate(&licensing, &Device::new(2), &test_key).await;
    activate(&licensing, &Device::new(3), &test_key).await;
    assert_eq!(Device::new(1).check(licensing.app(), &ordinary).await.status, StatusCode::OK);
    assert_eq!(Device::new(2).check(licensing.app(), &tested).await.status, StatusCode::OK);

    let numbers = overview(&licensing.admin).await;
    let counts: Vec<&Value> = ["unused", "redeemed", "revoked", "active_computers", "computers_checked_last_24h"]
        .iter()
        .map(|name| &numbers[*name])
        .collect();
    assert_eq!(counts, [&json!(0), &json!(1), &json!(0), &json!(1), &json!(1)], "{numbers}");
    assert_eq!(numbers["checks_last_24h"], 1, "only the ordinary licence's check");
}
