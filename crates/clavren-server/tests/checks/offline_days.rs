// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:25.630612
//+ Revised: 	2026/11/14 12:05:25.630612
// -----------------------------------------------------------------------------

//! How long a token lasts: the product's offline days.

use super::*;

/// Sets the product's offline days through the admin API.
async fn set_offline_days(licensing: &Licensing, days: i64) {
    let path = format!("/api/admin/products/{}", licensing.product_id);
    let reply = licensing.admin.call(axum::http::Method::PATCH, &path, Some(json!({ "offline_days": days }))).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn tokens_last_the_products_offline_days_at_activation_and_at_each_check(pool: PgPool) {
    let mut licensing = Licensing::new(pool).await;
    set_offline_days(&licensing, 7).await;
    let (_, key) = licensing.issue(json!({})).await;
    let reply = Device::new(1).activate(licensing.app(), PRODUCT, &key).await;
    let claims = verified(&reply, PRODUCT, licensing.now().timestamp());
    assert_eq!(claims.exp, claims.iat + 7 * 86_400, "activation");
    let activation = reply.body["activation"].as_str().unwrap().to_string();

    // a changed limit applies at the next check
    set_offline_days(&licensing, 365).await;
    licensing.admin.wait(Duration::from_secs(86_400)).await;
    let checked = Device::new(1).check(licensing.app(), &activation).await;
    let claims = verified(&checked, PRODUCT, licensing.now().timestamp());
    assert_eq!(claims.exp, claims.iat + 365 * 86_400, "check");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_sooner_licence_expiry_still_caps_the_products_offline_days(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    set_offline_days(&licensing, 7).await;
    let ends = licensing.now() + chrono::Duration::days(3);
    let (_, key) = licensing.issue(json!({ "expires_at": ends.to_rfc3339() })).await;
    let reply = Device::new(1).activate(licensing.app(), PRODUCT, &key).await;
    let claims = verified(&reply, PRODUCT, licensing.now().timestamp());
    assert_eq!(claims.exp, ends.timestamp());
}
