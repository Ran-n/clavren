// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.824867
//+ Revised: 	2026/11/03 08:25:52.804153
// -----------------------------------------------------------------------------

//! The daily check: challenges that work once, for two minutes and one activation, signatures
//! from the device key only, fresh tokens, and the answers after a reset, revoke, expiry or
//! deactivation.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use std::time::Duration;

use axum::http::StatusCode;
use clavren::maintenance;
use common::device::{Device, verified};
use common::licensing::{Licensing, PRODUCT};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// A licence activated on device 1; returns the licence ID and the activation ID.
async fn activated(licensing: &Licensing, extra: Value) -> (String, String) {
    let (licence, key) = licensing.issue(extra).await;
    let reply = Device::new(1).activate(licensing.app(), PRODUCT, &key).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    (licence, reply.body["activation"].as_str().unwrap().to_string())
}

async fn refusals(licensing: &Licensing) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT metadata->>'reason' FROM audit_logs WHERE action = 'check.refused' ORDER BY created_at, id",
    )
    .fetch_all(licensing.admin.pool())
    .await
    .unwrap()
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_signed_check_renews_the_token(pool: PgPool) {
    let mut licensing = Licensing::new(pool).await;
    let (licence, activation) = activated(&licensing, json!({})).await;
    licensing.admin.wait(Duration::from_secs(86_400)).await;

    let challenge = Device::new(1).challenge(licensing.app(), &activation).await;
    assert_eq!(challenge.status, StatusCode::OK, "{:?}", challenge.body);
    let fields: Vec<&str> = challenge.body.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(fields, ["challenge", "expires_at", "nonce", "server_time"]);
    let reply = Device::new(1)
        .send_check(licensing.app(), &activation, Device::new(1).check_body(&activation, &challenge.body))
        .await;

    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    let now = licensing.now().timestamp();
    let claims = verified(&reply, PRODUCT, now);
    assert_eq!((claims.act.to_string(), claims.exp), (activation.clone(), now + 30 * 86_400));
    let state = licensing.licence(&licence).await;
    assert_eq!(state["active_activation"]["app_version"], "1.0.1");
    assert_eq!(state["active_activation"]["last_checked_at"], json!(licensing.now()));
    let stored: Vec<u8> = sqlx::query_scalar("SELECT nonce_hash FROM verification_challenges")
        .fetch_one(licensing.admin.pool())
        .await
        .unwrap();
    let nonce = clavren_protocol::encoding::decode(challenge.body["nonce"].as_str().unwrap()).unwrap();
    assert_eq!(stored, blake3::hash(&nonce).as_bytes(), "only the nonce's hash is stored");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_challenge_works_once_and_only_for_two_minutes(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, activation) = activated(&licensing, json!({})).await;
    let device = Device::new(1);

    let challenge = device.challenge(licensing.app(), &activation).await.body;
    let body = device.check_body(&activation, &challenge);
    assert_eq!(device.send_check(licensing.app(), &activation, body.clone()).await.status, StatusCode::OK);
    let replay = device.send_check(licensing.app(), &activation, body).await;
    assert_eq!((replay.status, replay.error()), (StatusCode::UNAUTHORIZED, "challenge_invalid"));

    let late = device.challenge(licensing.app(), &activation).await.body;
    licensing.admin.fixture.clock.advance(Duration::from_secs(121));
    let reply = device.send_check(licensing.app(), &activation, device.check_body(&activation, &late)).await;
    assert_eq!(reply.error(), "challenge_invalid");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn only_the_device_key_can_answer_its_own_challenges(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, activation) = activated(&licensing, json!({})).await;
    let (_, other_key) = licensing.issue(json!({})).await;
    let thief = Device::new(2);
    let other_activation =
        thief.activate(licensing.app(), PRODUCT, &other_key).await.body["activation"].as_str().unwrap().to_string();
    let device = Device::new(1);

    let challenge = device.challenge(licensing.app(), &activation).await.body;
    let forged = thief.check_body(&activation, &challenge);
    assert_eq!(
        thief.send_check(licensing.app(), &activation, forged).await.error(),
        "challenge_invalid",
        "another device's key"
    );

    let foreign = thief.challenge(licensing.app(), &other_activation).await.body;
    let reply = device.send_check(licensing.app(), &activation, device.check_body(&activation, &foreign)).await;
    assert_eq!(reply.error(), "challenge_invalid", "another activation's challenge");

    let challenge = device.challenge(licensing.app(), &activation).await.body;
    let mut wrong_nonce = device.check_body(&activation, &challenge);
    wrong_nonce["nonce"] = json!(clavren_protocol::encoding::encode(&[0; 32]));
    assert_eq!(
        device.send_check(licensing.app(), &activation, wrong_nonce).await.error(),
        "challenge_invalid",
        "a different nonce"
    );
    let mut garbled = device.check_body(&activation, &device.challenge(licensing.app(), &activation).await.body);
    garbled["signature"] = json!("not-a-signature");
    assert_eq!(device.send_check(licensing.app(), &activation, garbled).await.error(), "challenge_invalid");
    assert_eq!(
        device.check(licensing.app(), &activation).await.status,
        StatusCode::OK,
        "the real device still checks in"
    );
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn after_a_reset_the_old_computer_is_refused(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, activation) = activated(&licensing, json!({})).await;
    let device = Device::new(1);
    let pending = device.challenge(licensing.app(), &activation).await.body;
    licensing.admin_post(&format!("/api/admin/licences/{licence}/reset"), json!({})).await;

    let reply = device.send_check(licensing.app(), &activation, device.check_body(&activation, &pending)).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::FORBIDDEN, "activation_revoked"));
    assert!(reply.body["server_time"].is_string());
    let reply = device.challenge(licensing.app(), &activation).await;
    assert_eq!(reply.error(), "activation_revoked", "no new challenges either");
    assert_eq!(refusals(&licensing).await, ["activation_revoked"]);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn revoked_expired_and_unavailable_licences_are_refused_and_audited(pool: PgPool) {
    let mut licensing = Licensing::new(pool).await;
    let device = Device::new(1);
    let (revoked, revoked_activation) = activated(&licensing, json!({})).await;
    let pending = device.challenge(licensing.app(), &revoked_activation).await.body;
    licensing.admin_post(&format!("/api/admin/licences/{revoked}/revoke"), json!({})).await;
    let reply =
        device.send_check(licensing.app(), &revoked_activation, device.check_body(&revoked_activation, &pending)).await;
    assert_eq!(reply.error(), "licence_revoked");

    let (expiring, _) =
        licensing.issue(json!({ "expires_at": (licensing.now() + chrono::Duration::days(2)).to_rfc3339() })).await;
    let expiring_key =
        licensing.admin_post(&format!("/api/admin/licences/{expiring}/reveal"), json!({})).await.body["key"]
            .as_str()
            .unwrap()
            .to_string();
    let other = Device::new(3);
    let expiring_activation =
        other.activate(licensing.app(), PRODUCT, &expiring_key).await.body["activation"].as_str().unwrap().to_string();
    let pending = other.challenge(licensing.app(), &expiring_activation).await.body;
    licensing.admin.wait(Duration::from_secs(3 * 86_400)).await;
    let pending_late = other.challenge(licensing.app(), &expiring_activation).await.body;
    let reply = other
        .send_check(licensing.app(), &expiring_activation, other.check_body(&expiring_activation, &pending_late))
        .await;
    assert_eq!(reply.error(), "licence_expired");
    assert_eq!(
        other
            .send_check(licensing.app(), &expiring_activation, other.check_body(&expiring_activation, &pending))
            .await
            .error(),
        "challenge_invalid"
    );

    let (_, live) = activated_with(&licensing, Device::new(4)).await;
    sqlx::query("UPDATE products SET is_active = false").execute(licensing.admin.pool()).await.unwrap();
    assert_eq!(Device::new(4).check(licensing.app(), &live).await.error(), "product_unavailable");
    assert_eq!(refusals(&licensing).await, ["licence_revoked", "licence_expired", "product_unavailable"]);
}

async fn activated_with(licensing: &Licensing, device: Device) -> (String, String) {
    let (licence, key) = licensing.issue(json!({})).await;
    let reply = device.activate(licensing.app(), PRODUCT, &key).await;
    (licence, reply.body["activation"].as_str().unwrap().to_string())
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn unknown_or_malformed_activations_are_refused(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let device = Device::new(1);
    for activation in [Uuid::now_v7().to_string(), "not-an-id".to_string()] {
        let reply = device.challenge(licensing.app(), &activation).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::FORBIDDEN, "activation_revoked"), "{activation}");
    }
    let body = json!({ "challenge": Uuid::now_v7(), "nonce": "AA", "signature": "AA", "app_version": "1" });
    let reply = device.send_check(licensing.app(), &Uuid::now_v7().to_string(), body).await;
    assert_eq!(reply.error(), "challenge_invalid");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn one_activation_gets_thirty_checks_an_hour(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, activation) = activated(&licensing, json!({})).await;
    let device = Device::new(1);
    for _ in 0..30 {
        assert_eq!(device.check(licensing.app(), &activation).await.status, StatusCode::OK);
    }
    let blocked = device.check(licensing.app(), &activation).await;
    assert_eq!((blocked.status, blocked.error()), (StatusCode::TOO_MANY_REQUESTS, "rate_limited"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn maintenance_deletes_used_and_expired_challenges(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, activation) = activated(&licensing, json!({})).await;
    let device = Device::new(1);
    device.check(licensing.app(), &activation).await;
    device.challenge(licensing.app(), &activation).await;
    licensing.admin.fixture.clock.advance(Duration::from_secs(60));
    device.challenge(licensing.app(), &activation).await;
    licensing.admin.fixture.clock.advance(Duration::from_secs(61));

    let cleaned = maintenance::run_once(&licensing.admin.fixture.state).await.unwrap();
    assert_eq!(cleaned.challenges, 2, "the used one and the expired one");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM verification_challenges")
        .fetch_one(licensing.admin.pool())
        .await
        .unwrap();
    assert_eq!(left, 1, "the one still valid stays");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn checks_are_counted_by_the_hour_and_old_hours_are_forgotten(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, activation) = activated(&licensing, json!({})).await;
    let device = Device::new(1);
    let clock = &licensing.admin.fixture.clock;
    for _ in 0..2 {
        assert_eq!(device.check(licensing.app(), &activation).await.status, StatusCode::OK);
    }
    clock.advance(Duration::from_secs(3600));
    assert_eq!(device.check(licensing.app(), &activation).await.status, StatusCode::OK);
    let counts: Vec<i64> = sqlx::query_scalar("SELECT checks FROM check_counts ORDER BY hour")
        .fetch_all(licensing.admin.pool())
        .await
        .unwrap();
    assert_eq!(counts, [2, 1], "one row per hour, not per check");

    // 30 days after the first checks: their hour began more than 30 days ago, the next one didn't.
    clock.advance(Duration::from_secs(30 * 24 * 3600 - 3600));
    let cleaned = maintenance::run_once(&licensing.admin.fixture.state).await.unwrap();
    assert_eq!(cleaned.check_counts, 1, "only the hour more than 30 days old");
}
