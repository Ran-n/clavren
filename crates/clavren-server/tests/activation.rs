// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.617002
//+ Revised: 	2026/11/09 22:30:55.486611
// -----------------------------------------------------------------------------

//! `POST /api/v1/activations`: binding a key to one computer, atomically and idempotently, with a
//! signed token, and every way it is refused.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use std::time::Duration;

use axum::http::{Method, StatusCode, header};
use clavren_protocol::check::device_hash;
use clavren_protocol::encoding;
use common::api::Call;
use common::device::{Device, verified};
use common::licensing::{Licensing, PRODUCT};
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_key_activates_and_gets_a_signed_token(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, key) = licensing.issue(json!({})).await;
    let device = Device::new(1);
    let reply = device.activate(licensing.app(), PRODUCT, &key.to_lowercase().replace('-', " ")).await;

    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");
    let fields: Vec<&str> = reply.body.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(fields, ["activation", "licence_expires_at", "server_time", "token", "token_expires_at"]);
    let now = licensing.now();
    let claims = verified(&reply, PRODUCT, now.timestamp());
    assert_eq!(
        (claims.sub.to_string(), claims.act.to_string()),
        (licence.clone(), reply.body["activation"].as_str().unwrap().to_string())
    );
    let public_key = encoding::decode_array::<32>(&device.public_key()).unwrap();
    assert_eq!(claims.dev, device_hash(&public_key));
    assert_eq!(
        (claims.iss.as_str(), claims.lic_exp, claims.iat),
        ("https://licences.example.com", None, now.timestamp())
    );
    assert_eq!(claims.exp, now.timestamp() + 30 * 86_400);
    assert_eq!(reply.body["server_time"], json!(now));

    let state = licensing.licence(&licence).await;
    assert_eq!(state["state"], "redeemed");
    assert_eq!(state["active_activation"]["device_name"], "PC 1");
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE action = 'activation.created' AND admin_user_id IS NULL",
    )
    .fetch_one(licensing.admin.pool())
    .await
    .unwrap();
    assert_eq!(audited, 1);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_same_computer_asking_again_gets_its_activation_back(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, key) = licensing.issue(json!({})).await;
    let device = Device::new(1);
    let first = device.activate(licensing.app(), PRODUCT, &key).await;
    let again = device.activate(licensing.app(), PRODUCT, &key).await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(again.body["activation"], first.body["activation"]);
    assert_ne!(again.body["token"], first.body["token"], "a fresh token");

    let other = Device::new(2).activate(licensing.app(), PRODUCT, &key).await;
    assert_eq!((other.status, other.error()), (StatusCode::CONFLICT, "already_activated"));
    assert!(other.body["server_time"].is_string());
}

/// Regression: activating again from the same computer kept the version of the first activation,
/// though the activation holds the version last reported.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn activating_again_records_the_new_app_version(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, key) = licensing.issue(json!({})).await;
    let device = Device::new(1);
    let first = device.activate(licensing.app(), PRODUCT, &key).await;
    let mut body = device.activation_body(PRODUCT, &key);
    body["app_version"] = json!("2.0.0");
    let again = Call::new(Method::POST, "/api/v1/activations").json(body).send(licensing.app()).await;
    assert_eq!(again.status, StatusCode::OK);
    let (version, checked): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT app_version, last_checked_at FROM activations WHERE id = $1::uuid")
            .bind(first.body["activation"].as_str().unwrap())
            .fetch_one(licensing.admin.pool())
            .await
            .unwrap();
    assert_eq!((version.as_str(), checked), ("2.0.0", Some(licensing.now())));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn unknown_malformed_and_foreign_keys_get_the_same_answer(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, key) = licensing.issue(json!({})).await;
    licensing.admin.create("/api/admin/products", json!({ "code": "other", "name": "Other" })).await;
    let device = Device::new(1);
    let mut messages = Vec::new();
    for (product, typed) in [(PRODUCT, "BRN-0000-0000-0000-0000"), ("other", key.as_str()), ("missing", key.as_str())] {
        let reply = device.activate(licensing.app(), product, typed).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::NOT_FOUND, "invalid_key"), "{product} {typed}");
        messages.push(reply.body["message"].clone());
    }
    assert!(messages.iter().all(|message| *message == messages[0]), "{messages:?}");
    let malformed = device.activate(licensing.app(), PRODUCT, "BRN-12").await;
    assert_eq!(malformed.error(), "invalid_key");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn revoked_expired_and_unavailable_licences_say_why(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let device = Device::new(1);
    let (revoked, revoked_key) = licensing.issue(json!({})).await;
    licensing.admin_post(&format!("/api/admin/licences/{revoked}/revoke"), json!({})).await;
    let reply = device.activate(licensing.app(), PRODUCT, &revoked_key).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::FORBIDDEN, "licence_revoked"));

    let ends = (licensing.now() + chrono::Duration::hours(1)).to_rfc3339();
    let (_, expiring_key) = licensing.issue(json!({ "expires_at": ends })).await;
    let (_, other_key) = licensing.issue(json!({})).await;
    licensing.admin.fixture.clock.advance(Duration::from_secs(3601));
    let reply = device.activate(licensing.app(), PRODUCT, &expiring_key).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::FORBIDDEN, "licence_expired"));

    sqlx::query("UPDATE products SET is_active = false").execute(licensing.admin.pool()).await.unwrap();
    let reply = device.activate(licensing.app(), PRODUCT, &other_key).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::FORBIDDEN, "product_unavailable"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_token_never_outlives_the_licence(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let ends = licensing.now() + chrono::Duration::days(3);
    let (_, key) = licensing.issue(json!({ "expires_at": ends.to_rfc3339() })).await;
    let reply = Device::new(1).activate(licensing.app(), PRODUCT, &key).await;
    let claims = verified(&reply, PRODUCT, licensing.now().timestamp());
    assert_eq!((claims.exp, claims.lic_exp), (ends.timestamp(), Some(ends.timestamp())));
    assert_eq!(reply.body["licence_expires_at"], json!(ends));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn many_computers_racing_for_one_key_leave_exactly_one_activation(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, key) = licensing.issue(json!({})).await;
    let devices: Vec<Device> = (1..=8).map(Device::new).collect();
    let attempts = devices.iter().map(|device| device.activate(licensing.app(), PRODUCT, &key));
    let replies = futures_join_all(attempts).await;

    let statuses: Vec<StatusCode> = replies.iter().map(|reply| reply.status).collect();
    assert_eq!(statuses.iter().filter(|status| **status == StatusCode::CREATED).count(), 1, "{statuses:?}");
    assert!(statuses.iter().all(|status| [StatusCode::CREATED, StatusCode::CONFLICT].contains(status)), "{statuses:?}");
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM activations").fetch_one(licensing.admin.pool()).await.unwrap();
    assert_eq!(rows, 1, "no orphan activations");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn bad_requests_are_refused_with_the_server_time(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, key) = licensing.issue(json!({})).await;
    let device = Device::new(1);
    let mut bodies = Vec::new();
    for (field, value) in [
        ("device_public_key", json!("too-short")),
        ("device_fingerprint", json!("AAAA")),
        ("device_name", json!("")),
        ("app_version", json!("x".repeat(201))),
        ("extra", json!(true)),
    ] {
        let mut body = device.activation_body(PRODUCT, &key);
        body[field] = value;
        bodies.push(body);
    }
    bodies.push(json!({ "product": PRODUCT }));
    for body in bodies {
        let reply = Call::new(Method::POST, "/api/v1/activations").json(body.clone()).send(licensing.app()).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{body}");
        assert!(reply.body["server_time"].is_string(), "{body}");
    }
    let huge = json!({ "padding": "x".repeat(20_000) });
    let reply = Call::new(Method::POST, "/api/v1/activations").json(huge).send(licensing.app()).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert!(
        reply.body["message"].as_str().unwrap().contains("length limit"),
        "a body over 16 KiB is refused unread: {:?}",
        reply.body
    );
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM activations").fetch_one(licensing.admin.pool()).await.unwrap();
    assert_eq!(rows, 0);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn one_address_gets_ten_activation_attempts_an_hour(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let device = Device::new(1);
    for _ in 0..10 {
        assert_eq!(device.activate(licensing.app(), PRODUCT, "BRN-0000-0000-0000-0000").await.error(), "invalid_key");
    }
    let (_, key) = licensing.issue(json!({})).await;
    let blocked = device.activate(licensing.app(), PRODUCT, &key).await;
    assert_eq!((blocked.status, blocked.error()), (StatusCode::TOO_MANY_REQUESTS, "rate_limited"));
    assert!(blocked.headers.contains_key(header::RETRY_AFTER));
    licensing.admin.fixture.clock.advance(Duration::from_secs(3600));
    assert_eq!(device.activate(licensing.app(), PRODUCT, &key).await.status, StatusCode::CREATED);
}

/// Runs every future at once on this task and collects their outputs in order.
async fn futures_join_all<F: std::future::Future>(futures: impl Iterator<Item = F>) -> Vec<F::Output> {
    let handles: Vec<_> = futures.map(Box::pin).collect();
    let mut outputs: Vec<Option<F::Output>> = handles.iter().map(|_| None).collect();
    let mut pending: Vec<_> = handles.into_iter().enumerate().collect();
    std::future::poll_fn(|context| {
        pending.retain_mut(|(index, future)| match future.as_mut().poll(context) {
            std::task::Poll::Ready(output) => {
                outputs[*index] = Some(output);
                false
            }
            std::task::Poll::Pending => true,
        });
        if pending.is_empty() { std::task::Poll::Ready(()) } else { std::task::Poll::Pending }
    })
    .await;
    outputs.into_iter().map(Option::unwrap).collect()
}
