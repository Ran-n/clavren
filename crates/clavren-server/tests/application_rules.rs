// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:49.159708
//+ Revised: 	2026/11/11 17:53:40.437543
// -----------------------------------------------------------------------------

//! The rules around licence applications: the rate limit, the web form's human check, bulk
//! decisions (all or nothing), the list's filters, decided ones kept as history, what deleting a customer
//! or a product takes with it, and that the admin endpoints need a session.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

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
async fn the_web_form_passes_the_human_check(pool: PgPool) {
    let applying = Applying::with_settings(pool, fake_turnstile().await).await;
    for token in [None, Some(""), Some("bad-answer"), Some(&*"x".repeat(2049))] {
        let reply = applying.apply(json!({ "source": "web", "turnstile_token": token })).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::FORBIDDEN, "human_check_failed"), "{token:?}");
    }
    let (id, _) = applying.applied(json!({ "source": "web", "turnstile_token": GOOD_ANSWER })).await;
    assert_eq!(applying.application(&id).await["source"], "web");
    // the failed checks counted against the address's five an hour
    assert_eq!(applying.apply(json!({ "email": "app@example.com" })).await.error(), "rate_limited");
    applying.licensing.admin.fixture.clock.advance(Duration::from_secs(3600));
    // an application's own request has no human check: the rate limit and the one-waiting rule hold it
    assert_eq!(applying.apply(json!({ "email": "app@example.com" })).await.status, StatusCode::CREATED);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_human_check_that_cannot_run_is_the_servers_fault(pool: PgPool) {
    let fake = fake_turnstile().await;
    let url = fake.applications.turnstile.as_ref().unwrap().verify_url.clone();
    let applying = Applying::with_settings(pool, turnstile_settings(&url, "a-wrong-secret")).await;
    let reply = applying.apply(json!({ "source": "web", "turnstile_token": GOOD_ANSWER })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_human_check_that_cannot_reach_cloudflare_takes_nothing(pool: PgPool) {
    let applying = Applying::with_settings(pool, turnstile_settings("http://127.0.0.1:9/siteverify", "secret")).await;
    let reply = applying.apply(json!({ "source": "web", "turnstile_token": GOOD_ANSWER })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"));
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM license_applications")
        .fetch_one(applying.admin().pool())
        .await
        .unwrap();
    assert_eq!(stored, 0);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn without_turnstile_keys_the_web_form_has_no_human_check(pool: PgPool) {
    let applying = Applying::new(pool).await;
    assert_eq!(applying.apply(json!({ "source": "web" })).await.status, StatusCode::CREATED);
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
async fn deleting_applications_keeps_their_licences(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (approved, secret) = applying.applied(json!({})).await;
    let licence = applying.approve(&approved, json!({})).await.body["licence"]["id"].as_str().unwrap().to_string();
    let (pending, _) = applying.applied(json!({ "email": "bo@example.com" })).await;
    let missing = uuid::Uuid::now_v7().to_string();
    let reply = applying
        .licensing
        .admin_post("/api/admin/applications/bulk/delete", json!({ "ids": [approved, missing] }))
        .await;
    assert_eq!((reply.status, &reply.body["ids"]), (StatusCode::NOT_FOUND, &json!([missing])));
    let reply =
        applying.licensing.admin_post("/api/admin/applications/bulk/delete", json!({ "ids": [approved] })).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let deleted = applying.admin().call(Method::DELETE, &format!("/api/admin/applications/{pending}"), None).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(applying.admin().get(&format!("/api/admin/applications/{pending}")).await.status, StatusCode::NOT_FOUND);
    assert_eq!(applying.status(&secret).await.error(), "application_unknown");
    assert_eq!(applying.licensing.licence(&licence).await["state"], "unused", "the licence stays");
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
async fn deleting_a_customer_erases_what_they_applied_with(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (id, _) = applying.applied(json!({})).await;
    let approved =
        applying.approve(&id, json!({ "customer": { "name": "Ana López", "email": "ana@example.com" } })).await;
    let customer = approved.body["licence"]["customer"]["id"].as_str().unwrap().to_string();
    let (other, _) = applying.applied(json!({ "email": "bo@example.com" })).await;
    let deleted = applying.admin().call(Method::DELETE, &format!("/api/admin/customers/{customer}"), None).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(applying.admin().get(&format!("/api/admin/applications/{id}")).await.status, StatusCode::NOT_FOUND);
    assert_eq!(applying.application(&other).await["state"], "pending", "someone else's stays");
    let found: i64 = sqlx::query_scalar("SELECT count(*) FROM license_applications WHERE email ILIKE 'ana%'")
        .fetch_one(applying.admin().pool())
        .await
        .unwrap();
    assert_eq!(found, 0);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn deleting_a_product_deletes_its_applications(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (approved, _) = applying.applied(json!({})).await;
    applying.approve(&approved, json!({})).await;
    applying.applied(json!({ "email": "bo@example.com" })).await;
    let path = format!("/api/admin/products/{}", applying.licensing.product_id);
    assert_eq!(applying.admin().call(Method::DELETE, &path, None).await.status, StatusCode::NO_CONTENT);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM license_applications")
        .fetch_one(applying.admin().pool())
        .await
        .unwrap();
    assert_eq!(left, 0);
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
