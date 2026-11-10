// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:17.229725
//+ Revised: 	2026/11/10 17:59:23.010544
// -----------------------------------------------------------------------------

//! `/api/admin/products` against a real database: creating, listing, editing and deleting
//! products (with their licences), with the rules on codes and the audit trail.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

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
            "code",
            "created_at",
            "description",
            "id",
            "is_active",
            "message_template",
            "name",
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
async fn a_code_is_made_from_the_name_when_none_is_given(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let typed = create(&admin, "typed", "Typed").await;
    let mut made = Vec::new();
    for body in [
        json!({ "name": "Ñandú Pro" }),
        json!({ "name": "Ñandú Pro", "code": "" }),
        json!({ "name": "Nandu pro", "code": "  " }),
        json!({ "name": "Nandu pro", "code": null }),
        json!({ "name": "!!!" }),
    ] {
        let reply = admin.call(Method::POST, "/api/admin/products", Some(body.clone())).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{body}: {:?}", reply.body);
        made.push(reply.body["code"].as_str().unwrap().to_string());
    }
    assert_eq!(made, ["nandu-pro", "nandu-pro-2", "nandu-pro-3", "nandu-pro-4", "product"]);
    let first = format!(
        "/api/admin/products/{}",
        id_of(
            &admin.get("/api/admin/products?limit=100").await.body["data"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["code"] == "nandu-pro")
                .unwrap()
                .clone()
        )
    );
    let renamed = admin.call(Method::PATCH, &first, Some(json!({ "name": "Something Else" }))).await;
    assert_eq!(renamed.body["code"], "nandu-pro", "renaming never changes the code");

    let audit = audit(&admin).await;
    let typed_entry = audit.iter().find(|(_, id, _)| id.map(|id| id.to_string()) == Some(id_of(&typed))).unwrap();
    assert_eq!(typed_entry.2, json!({}), "a typed code isn't marked");
    let marked = audit
        .iter()
        .filter(|(action, _, metadata)| action == "product.created" && *metadata == json!({ "automatic_code": true }))
        .count();
    assert_eq!(marked, 5);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_typed_code_that_is_taken_is_still_refused(pool: PgPool) {
    let admin = Admin::new(pool).await;
    create(&admin, "avalio", "Avalio").await;
    let reply =
        admin.call(Method::POST, "/api/admin/products", Some(json!({ "code": "avalio", "name": "Other" }))).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "product_code_taken"), "never suffixed silently");
    assert_eq!(audit(&admin).await.len(), 1, "no second entry");
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
async fn a_product_without_licences_is_deleted(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let unused = create(&admin, "unused", "Unused").await;
    let usage = admin.get(&format!("/api/admin/products/{}/usage", id_of(&unused))).await;
    assert_eq!((usage.status, usage.body), (StatusCode::OK, json!({ "licences": 0, "in_use": 0 })));
    let reply = admin.call(Method::DELETE, &format!("/api/admin/products/{}", id_of(&unused)), None).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let reply = admin.call(Method::GET, &format!("/api/admin/products/{}", id_of(&unused)), None).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::NOT_FOUND, "not_found"));
    let (action, _, metadata) = audit(&admin).await.pop().unwrap();
    assert_eq!((action.as_str(), metadata), ("product.deleted", json!({ "licences": 0, "in_use": 0 })));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_product_is_deleted_with_its_licences(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let doomed = create(&admin, "doomed", "Doomed").await;
    let other = create(&admin, "other", "Other").await;
    let issue = |product: &Value, quantity: usize, customer: Value| {
        let body = json!({ "product": id_of(product), "quantity": quantity, "customer": customer });
        let admin = &admin;
        async move { admin.create("/api/admin/licences", body).await }
    };
    let issued = issue(&doomed, 3, json!({ "name": "Ana" })).await;
    let ids: Vec<Uuid> =
        issued["licences"].as_array().unwrap().iter().map(|l| l["id"].as_str().unwrap().parse().unwrap()).collect();
    let kept = issue(&other, 1, Value::Null).await;
    sqlx::query(
        "INSERT INTO activations (license_id, device_public_key, device_name, platform, app_version)
         VALUES ($1, $2, 'PC', 'windows', '1.0.0')",
    )
    .bind(ids[1])
    .bind([9u8; 32].as_slice())
    .execute(admin.pool())
    .await
    .unwrap();
    admin.call(Method::POST, &format!("/api/admin/licences/{}/revoke", ids[2]), Some(json!({}))).await;

    let path = format!("/api/admin/products/{}", id_of(&doomed));
    let usage = admin.get(&format!("{path}/usage")).await;
    assert_eq!(usage.body, json!({ "licences": 3, "in_use": 1 }));
    let reply = admin.call(Method::DELETE, &path, None).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);

    let left = |sql: &'static str| {
        let ids = ids.clone();
        let admin = &admin;
        async move { sqlx::query_scalar::<_, i64>(sql).bind(ids).fetch_one(admin.pool()).await.unwrap() }
    };
    assert_eq!(left("SELECT count(*) FROM licenses WHERE id = ANY($1)").await, 0);
    assert_eq!(left("SELECT count(*) FROM license_key_secrets WHERE license_id = ANY($1)").await, 0);
    assert_eq!(left("SELECT count(*) FROM activations WHERE license_id = ANY($1)").await, 0);
    assert_eq!(admin.get(&path).await.status, StatusCode::NOT_FOUND);
    let kept_id = kept["licences"][0]["id"].as_str().unwrap();
    assert_eq!(
        admin.get(&format!("/api/admin/licences/{kept_id}")).await.status,
        StatusCode::OK,
        "other products keep theirs"
    );
    let customer = issued["customer"]["id"].as_str().unwrap();
    assert_eq!(admin.get(&format!("/api/admin/customers/{customer}")).await.status, StatusCode::OK, "customers stay");

    let (action, _, metadata) = audit(&admin).await.pop().unwrap();
    assert_eq!((action.as_str(), metadata), ("product.deleted", json!({ "licences": 3, "in_use": 1 })));
    let licences_deleted: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = 'licence.deleted'")
        .fetch_one(admin.pool())
        .await
        .unwrap();
    assert_eq!(licences_deleted, 3);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn deleting_or_counting_an_unknown_product_is_404(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let path = format!("/api/admin/products/{}", Uuid::now_v7());
    assert_eq!(admin.call(Method::DELETE, &path, None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(admin.get(&format!("{path}/usage")).await.status, StatusCode::NOT_FOUND);
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
