// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:49.159708
//+ Revised: 	2026/11/14 12:05:25.067404
// -----------------------------------------------------------------------------

//! The rules around licence applications: the rate limit, the web form's human check, bulk
//! decisions (all or nothing), the list's filters, decided ones kept as history, what deleting a customer
//! or a product takes with it, and that the admin endpoints need a session.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

#[path = "../common/mod.rs"]
mod common;
mod deletion;
mod human_check;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::api::Call;
use common::applying::{Applying, GOOD_ANSWER, fake_turnstile, turnstile_settings};
use serde_json::{Value, json};
use sqlx::PgPool;

fn ids(reply: &Value) -> Vec<String> {
    reply["data"].as_array().unwrap().iter().map(|row| row["id"].as_str().unwrap().to_string()).collect()
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn each_address_sends_five_applications_an_hour(pool: PgPool) {
    let applying = Applying::new(pool).await;
    for n in 0..5 {
        let reply = applying.apply(json!({ "email": format!("person{n}@example.com") })).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{n}: {:?}", reply.body);
    }
    let limited = applying.apply(json!({ "email": "sixth@example.com" })).await;
    assert_eq!((limited.status, limited.error()), (StatusCode::TOO_MANY_REQUESTS, "rate_limited"));
    assert!(limited.headers.contains_key("retry-after"));
    applying.licensing.admin.fixture.clock.advance(Duration::from_secs(3600));
    assert_eq!(applying.apply(json!({ "email": "later@example.com" })).await.status, StatusCode::CREATED);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn each_address_asks_about_applications_120_times_an_hour(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (_, secret) = applying.applied(json!({})).await;
    for n in 0..120 {
        assert_eq!(applying.status(&secret).await.status, StatusCode::OK, "{n}");
    }
    let limited = applying.status(&secret).await;
    assert_eq!((limited.status, limited.error()), (StatusCode::TOO_MANY_REQUESTS, "rate_limited"));
    assert!(limited.headers.contains_key("retry-after"));
    // a limit of its own: sending and the key page still answer
    assert_eq!(applying.apply(json!({ "email": "bo@example.com" })).await.status, StatusCode::CREATED);
    applying.licensing.admin.fixture.clock.advance(Duration::from_secs(3600));
    assert_eq!(applying.status(&secret).await.status, StatusCode::OK);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn bulk_rejection_is_all_or_nothing(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (first, _) = applying.applied(json!({})).await;
    let (second, _) = applying.applied(json!({ "email": "bo@example.com" })).await;
    let (decided, _) = applying.applied(json!({ "email": "cy@example.com" })).await;
    applying.reject(&decided, json!({})).await;
    let reply = applying
        .licensing
        .admin_post("/api/admin/applications/bulk/reject", json!({ "ids": [first, second, decided], "reason": "Full" }))
        .await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "application_decided"));
    assert_eq!(reply.body["ids"], json!([decided]));
    assert_eq!(applying.application(&first).await["state"], "pending", "nothing changed");

    let reply = applying
        .licensing
        .admin_post("/api/admin/applications/bulk/reject", json!({ "ids": [first, second, first], "reason": "Full" }))
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);
    for id in [&first, &second] {
        let application = applying.application(id).await;
        assert_eq!((&application["state"], &application["rejection_reason"]), (&json!("rejected"), &json!("Full")));
    }
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = 'application.rejected'")
        .fetch_one(applying.admin().pool())
        .await
        .unwrap();
    assert_eq!(audited, 3, "one entry per application, the repeated ID once");
    let empty = applying.licensing.admin_post("/api/admin/applications/bulk/reject", json!({ "ids": [] })).await;
    assert_eq!(empty.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_list_filters_by_state_product_and_search(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (ana, _) = applying.applied(json!({})).await;
    let (bo, _) = applying.applied(json!({ "name": "Bo Ek", "email": "bo@school.example" })).await;
    applying.reject(&bo, json!({})).await;
    let admin = applying.admin();
    let product = &applying.licensing.product_id;
    assert_eq!(ids(&admin.get("/api/admin/applications").await.body), [bo.clone(), ana.clone()], "newest first");
    assert_eq!(ids(&admin.get("/api/admin/applications?state=rejected").await.body), std::slice::from_ref(&bo));
    assert_eq!(ids(&admin.get("/api/admin/applications?q=school").await.body), std::slice::from_ref(&bo));
    assert_eq!(ids(&admin.get("/api/admin/applications?q=l%C3%B3pez").await.body), std::slice::from_ref(&ana));
    assert_eq!(ids(&admin.get(&format!("/api/admin/applications?product={product}&state=pending")).await.body), [ana]);
    let first = admin.get("/api/admin/applications?limit=1").await.body;
    let next = first["next_cursor"].as_str().unwrap();
    assert_eq!(ids(&admin.get(&format!("/api/admin/applications?limit=1&after={next}")).await.body).len(), 1);
    assert_eq!(admin.get("/api/admin/applications?state=lost").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(admin.get("/api/admin/applications?expand=customer").await.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn decided_applications_are_kept_as_history(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (rejected, rejected_secret) = applying.applied(json!({})).await;
    applying.reject(&rejected, json!({ "reason": "Only for schools" })).await;
    let (approved, _) = applying.applied(json!({ "email": "bo@example.com" })).await;
    assert_eq!(applying.approve(&approved, json!({ "customer": { "name": "Bo" } })).await.status, StatusCode::OK);
    applying.licensing.admin.fixture.clock.advance(Duration::from_secs(3 * 365 * 24 * 3600));
    clavren::maintenance::run_once(&applying.licensing.admin.fixture.state).await.unwrap();
    // read from the database: the admin's session ran out long ago
    let kept: Vec<(uuid::Uuid, Option<String>)> =
        sqlx::query_as("SELECT id, decision FROM license_applications ORDER BY id")
            .fetch_all(applying.admin().pool())
            .await
            .unwrap();
    let kept: Vec<(String, Option<String>)> =
        kept.into_iter().map(|(id, decision)| (id.to_string(), decision)).collect();
    assert_eq!(kept, [(rejected, Some("rejected".into())), (approved, Some("approved".into()))], "years later");
    let status = applying.status(&rejected_secret).await;
    assert_eq!((status.status, &status.body["reason"]), (StatusCode::OK, &json!("Only for schools")));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn every_application_endpoint_needs_a_session(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (id, _) = applying.applied(json!({})).await;
    let app = applying.licensing.app();
    for (method, path) in [
        (Method::GET, "/api/admin/applications".to_string()),
        (Method::GET, format!("/api/admin/applications/{id}")),
        (Method::POST, format!("/api/admin/applications/{id}/approve")),
        (Method::POST, format!("/api/admin/applications/{id}/reject")),
        (Method::DELETE, format!("/api/admin/applications/{id}")),
        (Method::POST, "/api/admin/applications/bulk/reject".to_string()),
        (Method::POST, "/api/admin/applications/bulk/delete".to_string()),
    ] {
        let reply = Call::new(method.clone(), &path).json(json!({ "ids": [id] })).send(app).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
    let browser = &applying.licensing.admin.browser;
    let no_csrf = Call::new(Method::POST, &format!("/api/admin/applications/{id}/reject"))
        .signed_in(browser)
        .csrf("wrong")
        .json(json!({}))
        .send(app)
        .await;
    assert_eq!(no_csrf.status, StatusCode::FORBIDDEN);
    assert_eq!(applying.application(&id).await["state"], "pending");
}

fn id_of(entity: &Value) -> String {
    entity["id"].as_str().expect("an entity with an id").to_string()
}
