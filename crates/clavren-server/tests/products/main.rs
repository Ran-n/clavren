// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:17.229725
//+ Revised: 	2026/11/14 18:09:54.573683
// -----------------------------------------------------------------------------

//! `/api/admin/products` against a real database: creating, listing, editing and deleting
//! products (with their licences), with the rules on codes and the audit trail.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod codes;
#[path = "../common/mod.rs"]
mod common;
mod deleting;
mod fields;
mod listing;

use axum::http::{Method, StatusCode, header};
use common::api::{Admin, Call};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn create(admin: &Admin, code: &str, name: &str) -> Value {
    admin.create("/api/admin/products", json!({ "code": code, "name": name, "test_licence": false })).await
}

async fn audit(admin: &Admin) -> Vec<(String, Option<Uuid>, Value)> {
    sqlx::query_as(
        "SELECT action, entity_id, metadata FROM audit_logs WHERE entity_type = 'product' ORDER BY created_at, id",
    )
    .fetch_all(admin.pool())
    .await
    .unwrap()
}

fn id_of(product: &Value) -> String {
    product["id"].as_str().unwrap().to_string()
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_created_product_comes_back_with_its_defaults(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let reply = admin
        .call(
            Method::POST,
            "/api/admin/products",
            Some(json!({ "code": "Avalio-Lite", "name": " Avalio Lite ", "description": "", "test_licence": false })),
        )
        .await;

    assert_eq!(reply.status, StatusCode::CREATED);
    assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");
    let product = &reply.body;
    assert_eq!(product["code"], "avalio-lite");
    assert_eq!(product["name"], "Avalio Lite");
    assert_eq!(product["description"], Value::Null);
    assert_eq!(product["is_active"], true);
    assert!(product["message_template"].as_str().unwrap().contains("{key}"));
    let keys: Vec<&str> = product.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "accepts_applications",
            "code",
            "created_at",
            "description",
            "id",
            "is_active",
            "key_instructions",
            "message_template",
            "name",
            "offline_days",
            "price",
            "test_licence",
            "updated_at"
        ]
    );
    assert_eq!(product["test_licence"], Value::Null, "none was asked for");

    let read = admin.call(Method::GET, &format!("/api/admin/products/{}", id_of(product)), None).await;
    let mut created = product.clone();
    created.as_object_mut().unwrap().remove("test_licence");
    assert_eq!(read.body, created);
    let audit = audit(&admin).await;
    assert_eq!(audit.len(), 1);
    assert_eq!((audit[0].0.as_str(), audit[0].1.unwrap().to_string()), ("product.created", id_of(product)));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn codes_are_unique_and_requests_are_checked(pool: PgPool) {
    let admin = Admin::new(pool).await;
    create(&admin, "avalio-lite", "Avalio Lite").await;
    let reply =
        admin.call(Method::POST, "/api/admin/products", Some(json!({ "code": "AVALIO-LITE", "name": "Again" }))).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "product_code_taken"));

    for body in [
        json!({ "code": "bad code", "name": "X" }),
        json!({ "code": "x", "name": "" }),
        json!({ "code": "x", "name": "X", "message_template": "Hi {name}" }),
        json!({ "code": "x", "name": "X", "colour": "red" }),
        json!({ "code": "x" }),
        json!({ "name": "" }),
        json!({ "name": "   " }),
        json!({ "name": "x".repeat(201) }),
    ] {
        let reply = admin.call(Method::POST, "/api/admin/products", Some(body.clone())).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{body}");
    }
    assert_eq!(audit(&admin).await.len(), 1);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn editing_changes_only_the_given_fields(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = create(&admin, "avalio-lite", "Avalio Lite").await;
    let path = format!("/api/admin/products/{}", id_of(&product));

    let reply =
        admin.call(Method::PATCH, &path, Some(json!({ "description": "For schools", "is_active": false }))).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(
        (reply.body["description"].as_str(), reply.body["is_active"].as_bool()),
        (Some("For schools"), Some(false))
    );
    assert_eq!(reply.body["name"], "Avalio Lite");

    let reply = admin.call(Method::PATCH, &path, Some(json!({ "description": null }))).await;
    assert_eq!(reply.body["description"], Value::Null);
    let blank = admin.call(Method::PATCH, &path, Some(json!({ "code": "" }))).await;
    assert_eq!(
        (blank.status, blank.error()),
        (StatusCode::BAD_REQUEST, "invalid_request"),
        "a code is only made on create"
    );
    let unchanged = admin.call(Method::PATCH, &path, Some(json!({ "name": "Avalio Lite" }))).await;
    assert_eq!(unchanged.body, reply.body);

    let audit = audit(&admin).await;
    let fields: Vec<&Value> = audit.iter().skip(1).map(|(_, _, metadata)| &metadata["fields"]).collect();
    assert_eq!(fields, [&json!(["description", "is_active"]), &json!(["description"])], "no entry for a no-op");
}

async fn insert_licence(admin: &Admin, product: &Value) {
    sqlx::query("INSERT INTO licenses (product_id, key_lookup) VALUES ($1::uuid, $2)")
        .bind(id_of(product))
        .bind(Uuid::now_v7().as_bytes().repeat(2))
        .execute(admin.fixture.pool())
        .await
        .unwrap();
}
