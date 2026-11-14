// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.689302
//+ Revised: 	2026/11/14 12:05:24.287323
// -----------------------------------------------------------------------------

//! The admin endpoints behind the dashboard's other pages: activations, the overview, the audit
//! log, backups and preferences.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod audit_and_preferences;
#[path = "../common/mod.rs"]
mod common;
mod overview;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::device::Device;
use common::licensing::{Licensing, PRODUCT};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn activated(licensing: &Licensing, device: u8) -> (String, String) {
    let (licence, key) = licensing.issue(json!({})).await;
    let reply = Device::new(device).activate(licensing.app(), PRODUCT, &key).await;
    (licence, reply.body["activation"].as_str().unwrap().to_string())
}

fn ids(reply: &Value) -> Vec<String> {
    reply["data"].as_array().unwrap().iter().map(|row| row["id"].as_str().unwrap().to_string()).collect()
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn activations_are_listed_filtered_and_expanded(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (first_licence, first) = activated(&licensing, 1).await;
    let (_, second) = activated(&licensing, 2).await;
    licensing.admin_post(&format!("/api/admin/licences/{first_licence}/reset"), json!({})).await;

    let all = licensing.admin.get("/api/admin/activations").await;
    assert_eq!(ids(&all.body), [second.clone(), first.clone()], "newest first");
    let row = &all.body["data"][1];
    assert_eq!(
        (row["licence"].as_str(), row["revocation_reason"].as_str()),
        (Some(first_licence.as_str()), Some("reset"))
    );
    assert!(row.get("device_public_key").is_none(), "device keys stay inside the server");

    assert_eq!(
        ids(&licensing.admin.get("/api/admin/activations?state=active").await.body),
        std::slice::from_ref(&second)
    );
    assert_eq!(
        ids(&licensing.admin.get(&format!("/api/admin/activations?licence={first_licence}")).await.body),
        [first]
    );
    assert_eq!(ids(&licensing.admin.get("/api/admin/activations?q=pc%202").await.body), std::slice::from_ref(&second));
    let expanded = licensing.admin.get("/api/admin/activations?state=active&expand=licence").await.body;
    assert_eq!(expanded["data"][0]["licence"]["state"], "redeemed");
    assert!(expanded["data"][0]["licence"]["product"].is_string(), "only what was asked for");
    let nested =
        licensing.admin.get("/api/admin/activations?state=active&expand=licence.product,licence.customer").await.body;
    assert_eq!(nested["data"][0]["licence"]["product"]["code"], PRODUCT);
    assert_eq!(nested["data"][0]["licence"]["customer"], Value::Null, "issued without a customer");
    assert_eq!(licensing.admin.get("/api/admin/activations?expand=product").await.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn revoking_an_activation_resets_its_licence(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, activation) = activated(&licensing, 1).await;
    let reply = licensing.admin_post(&format!("/api/admin/activations/{activation}/revoke"), json!({})).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["revocation_reason"], "reset");
    assert_eq!(licensing.licence(&licence).await["state"], "unused");

    let again = licensing.admin_post(&format!("/api/admin/activations/{activation}/revoke"), json!({})).await;
    assert_eq!((again.status, again.error()), (StatusCode::CONFLICT, "activation_revoked"));
    let unknown =
        licensing.admin_post(&format!("/api/admin/activations/{}/revoke", uuid::Uuid::now_v7()), json!({})).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
}

/// Regression: revoking an activation reset its licence, ending whichever computer was using it by
/// then. When the licence had moved to another computer between reading the activation and the
/// reset (another admin's reset, then a new activation), that other computer was ended instead.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn ending_an_activation_never_ends_the_computer_that_took_its_place(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, key) = licensing.issue(json!({})).await;
    let first = Device::new(1).activate(licensing.app(), PRODUCT, &key).await.body["activation"].clone();
    licensing.admin_post(&format!("/api/admin/licences/{licence}/reset"), json!({})).await;
    let second = Device::new(2).activate(licensing.app(), PRODUCT, &key).await.body["activation"].clone();

    let state = &licensing.admin.fixture.state;
    let actor = clavren::audit::Actor {
        admin_id: licensing.admin.fixture.admin.id,
        ip: common::api::client().ip(),
        at: licensing.now(),
    };
    let licence_id: uuid::Uuid = licence.parse().unwrap();
    let first_id: uuid::Uuid = first.as_str().unwrap().parse().unwrap();
    let refused = clavren::licences::end_activation(state, &actor, licence_id, first_id).await.unwrap_err();
    assert_eq!(refused.code(), "activation_revoked");
    let activations = licensing.admin.get("/api/admin/activations?state=active").await.body;
    assert_eq!(activations["data"][0]["id"], second, "the second computer still runs");
    assert_eq!(licensing.licence(&licence).await["state"], "redeemed");

    // an activation is only ever ended through its own licence
    let (other, _) = licensing.issue(json!({})).await;
    let second_id: uuid::Uuid = second.as_str().unwrap().parse().unwrap();
    let mismatched =
        clavren::licences::end_activation(state, &actor, other.parse().unwrap(), second_id).await.unwrap_err();
    assert_eq!(mismatched.code(), "activation_revoked");
    assert_eq!(licensing.licence(&licence).await["state"], "redeemed", "the second computer still runs");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn every_console_endpoint_needs_a_session(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    for path in [
        "/api/admin/activations",
        "/api/admin/overview",
        "/api/admin/audit",
        "/api/admin/backups",
        "/api/admin/preferences",
    ] {
        let reply = common::api::Call::new(Method::GET, path).send(licensing.app()).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{path}");
    }
}

fn urlencode(text: &str) -> String {
    form_urlencoded::byte_serialize(text.as_bytes()).collect()
}
