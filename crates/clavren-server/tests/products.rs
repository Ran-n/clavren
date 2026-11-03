// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:17.229725
//+ Revised: 	2026/11/03 04:54:17.229725
// -----------------------------------------------------------------------------

//! `/api/admin/products` against a real database: creating, listing, editing and deleting
//! products, with the rules on codes and the audit trail.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use axum::http::{Method, StatusCode, header};
use common::api::{Admin, Call};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn create(admin: &Admin, code: &str, name: &str) -> Value {
    admin.create("/api/admin/products", json!({ "code": code, "name": name })).await
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
            Some(json!({ "code": "Avalio-Lite", "name": " Avalio Lite ", "description": "" })),
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
        ["code", "created_at", "description", "id", "is_active", "message_template", "name", "updated_at"]
    );

    let read = admin.call(Method::GET, &format!("/api/admin/products/{}", id_of(product)), None).await;
    assert_eq!(read.body, *product);
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
    let unchanged = admin.call(Method::PATCH, &path, Some(json!({ "name": "Avalio Lite" }))).await;
    assert_eq!(unchanged.body, reply.body);

    let audit = audit(&admin).await;
    let fields: Vec<&Value> = audit.iter().skip(1).map(|(_, _, metadata)| &metadata["fields"]).collect();
    assert_eq!(fields, [&json!(["description", "is_active"]), &json!(["description"])], "no entry for a no-op");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_code_is_locked_once_the_product_has_licences(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = create(&admin, "avalio-lite", "Avalio Lite").await;
    create(&admin, "other", "Other").await;
    let path = format!("/api/admin/products/{}", id_of(&product));

    let reply = admin.call(Method::PATCH, &path, Some(json!({ "code": "other" }))).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "product_code_taken"));
    let reply = admin.call(Method::PATCH, &path, Some(json!({ "code": "avalio" }))).await;
    assert_eq!(reply.body["code"], "avalio", "free to change without licences");

    insert_licence(&admin, &product).await;
    let reply = admin.call(Method::PATCH, &path, Some(json!({ "code": "avalio-lite" }))).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "product_code_locked"));
    let reply = admin.call(Method::PATCH, &path, Some(json!({ "name": "Avalio" }))).await;
    assert_eq!(reply.status, StatusCode::OK, "everything else stays editable");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn only_a_product_without_licences_can_be_deleted(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let kept = create(&admin, "kept", "Kept").await;
    let unused = create(&admin, "unused", "Unused").await;
    insert_licence(&admin, &kept).await;

    let reply = admin.call(Method::DELETE, &format!("/api/admin/products/{}", id_of(&kept)), None).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "product_has_licences"));
    let reply = admin.call(Method::DELETE, &format!("/api/admin/products/{}", id_of(&unused)), None).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let reply = admin.call(Method::GET, &format!("/api/admin/products/{}", id_of(&unused)), None).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::NOT_FOUND, "not_found"));
    assert_eq!(audit(&admin).await.last().unwrap().0, "product.deleted");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn unknown_or_malformed_ids_are_not_found(pool: PgPool) {
    let admin = Admin::new(pool).await;
    for path in [format!("/api/admin/products/{}", Uuid::now_v7()), "/api/admin/products/not-an-id".to_string()] {
        for (method, body) in
            [(Method::GET, None), (Method::PATCH, Some(json!({ "name": "X" }))), (Method::DELETE, None)]
        {
            let reply = admin.call(method.clone(), &path, body).await;
            assert_eq!((reply.status, reply.error()), (StatusCode::NOT_FOUND, "not_found"), "{method} {path}");
        }
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_list_pages_by_name_and_filters_by_state(pool: PgPool) {
    let admin = Admin::new(pool).await;
    for (code, name) in [("c", "charlie"), ("a", "Alpha"), ("b", "bravo"), ("d", "Delta"), ("e", "echo")] {
        create(&admin, code, name).await;
    }
    let echo = admin.call(Method::GET, "/api/admin/products?limit=100", None).await.body["data"][4].clone();
    admin
        .call(Method::PATCH, &format!("/api/admin/products/{}", id_of(&echo)), Some(json!({ "is_active": false })))
        .await;

    let mut names = Vec::new();
    let mut path = "/api/admin/products?limit=2".to_string();
    loop {
        let reply = admin.call(Method::GET, &path, None).await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
        names.extend(
            reply.body["data"].as_array().unwrap().iter().map(|product| product["name"].as_str().unwrap().to_string()),
        );
        match reply.body["next_cursor"].as_str() {
            Some(cursor) => path = format!("/api/admin/products?limit=2&after={cursor}"),
            None => break,
        }
    }
    assert_eq!(names, ["Alpha", "bravo", "charlie", "Delta", "echo"]);

    let inactive = admin.call(Method::GET, "/api/admin/products?active=false", None).await;
    assert_eq!(inactive.body["data"].as_array().unwrap().len(), 1);
    assert_eq!(inactive.body["next_cursor"], Value::Null);
    for bad in ["?limit=0", "?limit=101", "?after=garbage", "?active=maybe", "?sort=name"] {
        let reply = admin.call(Method::GET, &format!("/api/admin/products{bad}"), None).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{bad}");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn every_endpoint_needs_a_session_and_changes_need_csrf(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = create(&admin, "a", "A").await;
    let path = format!("/api/admin/products/{}", id_of(&product));
    for (method, path) in [(Method::GET, "/api/admin/products"), (Method::GET, path.as_str())] {
        let reply = Call::new(method, path).send(&admin.fixture.app).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    }
    for (method, path) in
        [(Method::POST, "/api/admin/products"), (Method::PATCH, path.as_str()), (Method::DELETE, path.as_str())]
    {
        let reply = Call::new(method.clone(), path).json(json!({})).send(&admin.fixture.app).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{method} {path}");
        let reply = Call::new(method.clone(), path)
            .signed_in(&admin.browser)
            .csrf("")
            .json(json!({}))
            .send(&admin.fixture.app)
            .await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{method} {path}");
    }
}

async fn insert_licence(admin: &Admin, product: &Value) {
    sqlx::query("INSERT INTO licenses (product_id, key_lookup) VALUES ($1::uuid, $2)")
        .bind(id_of(product))
        .bind(Uuid::now_v7().as_bytes().repeat(2))
        .execute(admin.fixture.pool())
        .await
        .unwrap();
}
