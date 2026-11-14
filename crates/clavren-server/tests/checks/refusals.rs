// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:25.511865
//+ Revised: 	2026/11/14 12:05:25.511865
// -----------------------------------------------------------------------------

//! Checks that are refused: after a reset, a revocation, an expiry, a deletion, or made up.

use super::*;

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

/// Regression: every check counted against its activation ID before anything looked at it, so
/// made-up IDs each added a counter the server kept in memory for an hour.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn made_up_activations_add_no_counters(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let device = Device::new(1);
    let limits = &licensing.admin.fixture.state.public_limits.checks_per_activation;
    for _ in 0..5 {
        let body = json!({ "challenge": Uuid::now_v7(), "nonce": "AA", "signature": "AA", "app_version": "1" });
        let reply = device.send_check(licensing.app(), &Uuid::now_v7().to_string(), body).await;
        assert_eq!(reply.error(), "challenge_invalid");
    }
    assert_eq!(limits.tracked_keys(), 0);

    let (_, activation) = activated(&licensing, json!({})).await;
    assert_eq!(device.check(licensing.app(), &activation).await.status, StatusCode::OK);
    assert_eq!(limits.tracked_keys(), 1, "a real check is counted");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_deleted_licences_computer_locks(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, activation) = activated(&licensing, json!({})).await;
    let reply = licensing.admin.call(axum::http::Method::DELETE, &format!("/api/admin/licences/{licence}"), None).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);

    let reply = Device::new(1).challenge(licensing.app(), &activation).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::FORBIDDEN, "activation_revoked"), "the same as a reset");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_deleted_products_computers_lock(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, activation) = activated(&licensing, json!({})).await;
    let (_, spare_key) = licensing.issue(json!({})).await;
    let path = format!("/api/admin/products/{}", licensing.product_id);
    assert_eq!(licensing.admin.call(axum::http::Method::DELETE, &path, None).await.status, StatusCode::NO_CONTENT);

    let reply = Device::new(1).challenge(licensing.app(), &activation).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::FORBIDDEN, "activation_revoked"));
    let reply = Device::new(2).activate(licensing.app(), PRODUCT, &spare_key).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::NOT_FOUND, "invalid_key"));
}
