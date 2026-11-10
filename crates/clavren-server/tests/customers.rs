// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.910282
//+ Revised: 	2026/11/09 22:30:56.032679
// -----------------------------------------------------------------------------

//! `/api/admin/customers` against a real database: creating, searching, editing and deleting
//! customers, and that deleting one leaves nothing personal behind.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use axum::http::{Method, StatusCode};
use common::api::{Admin, Call};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn create(admin: &Admin, body: Value) -> Value {
    admin.create("/api/admin/customers", body).await
}

async fn names(admin: &Admin, query: &str) -> Vec<String> {
    let reply = admin.get(&format!("/api/admin/customers{query}")).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    reply.body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|customer| customer["name"].as_str().unwrap().to_string())
        .collect()
}

fn path_of(customer: &Value) -> String {
    format!("/api/admin/customers/{}", customer["id"].as_str().unwrap())
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_customer_is_created_read_and_edited(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let customer = create(&admin, json!({ "name": " Ana López ", "email": "ana@example.com" })).await;
    assert_eq!((customer["name"].as_str(), customer["notes"].clone()), (Some("Ana López"), Value::Null));
    let keys: Vec<&str> = customer.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["created_at", "email", "id", "name", "notes", "updated_at"]);

    assert_eq!(admin.call(Method::GET, &path_of(&customer), None).await.body, customer);
    let edited = admin
        .call(Method::PATCH, &path_of(&customer), Some(json!({ "email": null, "notes": "Paid by transfer" })))
        .await;
    assert_eq!(edited.status, StatusCode::OK);
    assert_eq!((edited.body["email"].clone(), edited.body["notes"].as_str()), (Value::Null, Some("Paid by transfer")));
    assert_eq!(edited.body["name"], "Ana López");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn emails_are_unique_ignoring_case(pool: PgPool) {
    let admin = Admin::new(pool).await;
    create(&admin, json!({ "name": "Ana", "email": "ana@example.com" })).await;
    let other = create(&admin, json!({ "name": "Ana B" })).await;

    let reply = admin
        .call(Method::POST, "/api/admin/customers", Some(json!({ "name": "Ana C", "email": "ANA@example.com" })))
        .await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "customer_email_taken"));
    let reply = admin.call(Method::PATCH, &path_of(&other), Some(json!({ "email": "Ana@Example.com" }))).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "customer_email_taken"));
    create(&admin, json!({ "name": "No email either" })).await;
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn invalid_customers_are_refused(pool: PgPool) {
    let admin = Admin::new(pool).await;
    for body in [
        json!({ "name": "" }),
        json!({ "name": "Ana", "email": "ana" }),
        json!({ "email": "a@b.co" }),
        json!({ "name": "A", "phone": "1" }),
    ] {
        let reply = admin.call(Method::POST, "/api/admin/customers", Some(body.clone())).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{body}");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn search_matches_name_or_email_and_takes_its_text_literally(pool: PgPool) {
    let admin = Admin::new(pool).await;
    create(&admin, json!({ "name": "Ana López", "email": "ana@school.example" })).await;
    create(&admin, json!({ "name": "Bruno", "email": "bruno@home.example" })).await;
    create(&admin, json!({ "name": "100% Pure" })).await;

    assert_eq!(names(&admin, "?q=SCHOOL").await, ["Ana López"]);
    assert_eq!(names(&admin, "?q=lópez").await, ["Ana López"]);
    assert_eq!(names(&admin, "?q=%25").await, ["100% Pure"], "% is a character, not a wildcard");
    assert_eq!(names(&admin, "?q=_").await, Vec::<String>::new());
    assert_eq!(names(&admin, "?q=%20").await.len(), 3, "a blank search shows everyone");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn lists_page_by_name_or_newest_first(pool: PgPool) {
    let admin = Admin::new(pool).await;
    for name in ["carla", "Ana", "bruno", "Dani"] {
        create(&admin, json!({ "name": name })).await;
    }
    let mut collected = Vec::new();
    for order in ["name", "newest"] {
        let mut names = Vec::new();
        let mut query = format!("?order={order}&limit=3");
        loop {
            let reply = admin.call(Method::GET, &format!("/api/admin/customers{query}"), None).await;
            names
                .extend(reply.body["data"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_string()));
            let Some(cursor) = reply.body["next_cursor"].as_str() else { break };
            query = format!("?order={order}&limit=3&after={cursor}");
        }
        collected.push(names);
    }
    assert_eq!(collected[0], ["Ana", "bruno", "carla", "Dani"]);
    assert_eq!(collected[1], ["Dani", "bruno", "Ana", "carla"]);

    let first = admin.call(Method::GET, "/api/admin/customers?limit=1", None).await;
    let cursor = first.body["next_cursor"].as_str().unwrap();
    let mixed = admin.call(Method::GET, &format!("/api/admin/customers?order=newest&after={cursor}"), None).await;
    assert_eq!((mixed.status, mixed.error()), (StatusCode::BAD_REQUEST, "invalid_request"));
    let unknown = admin.call(Method::GET, "/api/admin/customers?order=email", None).await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn deleting_a_customer_keeps_their_licences_and_erases_them(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let customer = create(&admin, json!({ "name": "Ana López", "email": "ana@example.com", "notes": "VIP" })).await;
    admin.call(Method::PATCH, &path_of(&customer), Some(json!({ "name": "Ana M. López" }))).await;
    let product: Uuid = sqlx::query_scalar("INSERT INTO products (code, name) VALUES ('p', 'P') RETURNING id")
        .fetch_one(admin.fixture.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO licenses (product_id, customer_id, key_lookup) VALUES ($1, $2::uuid, $3)")
        .bind(product)
        .bind(customer["id"].as_str().unwrap())
        .bind([7u8; 32].as_slice())
        .execute(admin.fixture.pool())
        .await
        .unwrap();

    let reply = admin.call(Method::DELETE, &path_of(&customer), None).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert_eq!(admin.call(Method::GET, &path_of(&customer), None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(admin.call(Method::DELETE, &path_of(&customer), None).await.status, StatusCode::NOT_FOUND);

    let licence_customer: Option<Uuid> =
        sqlx::query_scalar("SELECT customer_id FROM licenses").fetch_one(admin.fixture.pool()).await.unwrap();
    assert_eq!(licence_customer, None, "the licence stays, without the link");
    let everything: String = sqlx::query_scalar(
        "SELECT concat_ws(' ', (SELECT string_agg(a::text, ' ') FROM audit_logs a), (SELECT string_agg(c::text, ' ') FROM customers c))",
    )
    .fetch_one(admin.fixture.pool())
    .await
    .unwrap();
    for personal in ["Ana", "López", "ana@example.com", "VIP"] {
        assert!(!everything.contains(personal), "{personal} left behind: {everything}");
    }
    let actions: Vec<String> =
        sqlx::query_scalar("SELECT action FROM audit_logs WHERE entity_type = 'customer' ORDER BY created_at, id")
            .fetch_all(admin.fixture.pool())
            .await
            .unwrap();
    assert_eq!(actions, ["customer.created", "customer.updated", "customer.deleted"]);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn customer_endpoints_need_a_session_and_csrf(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let customer = create(&admin, json!({ "name": "Ana" })).await;
    let reply = Call::new(Method::GET, "/api/admin/customers").send(&admin.fixture.app).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    let reply = Call::new(Method::DELETE, &path_of(&customer))
        .signed_in(&admin.browser)
        .csrf("")
        .send(&admin.fixture.app)
        .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
}

/// Regression: a customer picked for a licence wasn't locked, so deleting them at the same moment
/// made the licence point at nothing (a server error) or deadlocked with the licence edit.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_customer_being_given_to_licences_cant_be_deleted_meanwhile(pool: PgPool) {
    let admin = Admin::new(pool.clone()).await;
    let customer = create(&admin, json!({ "name": "Ada" })).await;
    let id = Uuid::parse_str(customer["id"].as_str().unwrap()).unwrap();
    let mut picking = pool.begin().await.unwrap();
    assert!(clavren::customers::repository::find_for_share(&mut picking, id).await.unwrap().is_some());

    let mut deleting = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL lock_timeout = '200ms'").execute(&mut *deleting).await.unwrap();
    let error = sqlx::query("DELETE FROM customers WHERE id = $1").bind(id).execute(&mut *deleting).await.unwrap_err();
    assert_eq!(error.as_database_error().and_then(|error| error.code()).as_deref(), Some("55P03"), "lock timeout");
    picking.commit().await.unwrap();
}
