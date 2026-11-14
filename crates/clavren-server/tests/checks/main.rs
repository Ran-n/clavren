// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.824867
//+ Revised: 	2026/11/14 12:05:25.808016
// -----------------------------------------------------------------------------

//! The daily check: challenges that work once, for two minutes and one activation, signatures
//! from the device key only, fresh tokens, and the answers after a reset, revoke, delete, expiry
//! or deactivation.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

#[path = "../common/mod.rs"]
mod common;
mod housekeeping;
mod offline_days;
mod refusals;

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

async fn activated_with(licensing: &Licensing, device: Device) -> (String, String) {
    let (licence, key) = licensing.issue(json!({})).await;
    let reply = device.activate(licensing.app(), PRODUCT, &key).await;
    (licence, reply.body["activation"].as_str().unwrap().to_string())
}

/// A check and a reset (or a revoke) arriving together end consistent whichever wins: the check
/// either got its token before the computer was let go or was refused, and afterwards the
/// computer is refused for good.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_reset_or_a_revoke_racing_a_check_ends_consistent(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    for (seed, action, refusal, state) in
        [(1, "reset", "activation_revoked", "unused"), (2, "revoke", "licence_revoked", "revoked")]
    {
        let device = Device::new(seed);
        let (licence, activation) = activated_with(&licensing, Device::new(seed)).await;
        let challenge = device.challenge(licensing.app(), &activation).await.body;
        let path = format!("/api/admin/licences/{licence}/{action}");
        let (check, ended) = tokio::join!(
            device.send_check(licensing.app(), &activation, device.check_body(&activation, &challenge)),
            licensing.admin_post(&path, json!({})),
        );
        assert_eq!(ended.status, StatusCode::OK, "{action}: {:?}", ended.body);
        assert!(
            check.status == StatusCode::OK || (check.status == StatusCode::FORBIDDEN && check.error() == refusal),
            "{action}: {:?}",
            check.body
        );
        let later = device.challenge(licensing.app(), &activation).await;
        assert_eq!((later.status, later.error()), (StatusCode::FORBIDDEN, "activation_revoked"), "{action}");
        assert_eq!(licensing.licence(&licence).await["state"], state, "{action}");
        let active: i64 = sqlx::query_scalar("SELECT count(*) FROM activations WHERE revoked_at IS NULL")
            .fetch_one(licensing.admin.pool())
            .await
            .unwrap();
        assert_eq!(active, 0, "{action}");
    }
}
