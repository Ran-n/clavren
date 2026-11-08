// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.689302
//+ Revised: 	2026/11/07 19:37:00.319082
// -----------------------------------------------------------------------------

//! The admin endpoints behind the dashboard's other pages: activations, the overview, the audit
//! log, backups and preferences.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

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

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_overview_counts_and_warns_about_backups(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, activation) = activated(&licensing, 1).await;
    for _ in 0..2 {
        assert_eq!(Device::new(1).check(licensing.app(), &activation).await.status, StatusCode::OK);
    }
    licensing.issue(json!({})).await;
    let (revoked, _) = licensing.issue(json!({})).await;
    licensing.admin_post(&format!("/api/admin/licences/{revoked}/revoke"), json!({})).await;

    let overview = licensing.admin.get("/api/admin/overview").await.body;
    for (field, count) in [
        ("active_products", 1),
        ("inactive_products", 0),
        ("unused", 1),
        ("redeemed", 1),
        ("revoked", 1),
        ("active_computers", 1),
        ("computers_checked_last_24h", 1),
        ("checks_last_24h", 2),
    ] {
        assert_eq!(overview[field], count, "{field}");
    }
    assert_eq!((overview["last_backup"].clone(), overview["backup_overdue"].clone()), (Value::Null, json!(true)));
    let deleted =
        licensing.admin.call(axum::http::Method::DELETE, &format!("/api/admin/licences/{revoked}"), None).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let overview = licensing.admin.get("/api/admin/overview").await.body;
    assert_eq!(
        (overview["revoked"].clone(), overview["unused"].clone()),
        (json!(0), json!(1)),
        "a deleted licence counts nowhere"
    );

    sqlx::query(
        "INSERT INTO backup_runs (finished_at, archive_name, size_bytes) VALUES ($1, 'clavren-20270115.tar', 2048)",
    )
    .bind(licensing.now() - chrono::Duration::hours(3))
    .execute(licensing.admin.pool())
    .await
    .unwrap();
    let overview = licensing.admin.get("/api/admin/overview").await.body;
    assert_eq!(
        (overview["last_backup"]["size_bytes"].clone(), overview["backup_overdue"].clone()),
        (json!(2048), json!(false))
    );
    let backups = licensing.admin.get("/api/admin/backups").await.body;
    assert_eq!(backups["data"][0]["archive_name"], "clavren-20270115.tar");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_overview_warns_when_the_last_backup_is_two_days_old(pool: PgPool) {
    let mut licensing = Licensing::new(pool).await;
    sqlx::query("INSERT INTO backup_runs (finished_at, archive_name, size_bytes) VALUES ($1, 'old.tar', 1)")
        .bind(licensing.now())
        .execute(licensing.admin.pool())
        .await
        .unwrap();
    licensing.admin.wait(Duration::from_secs(49 * 3600)).await;
    assert_eq!(licensing.admin.get("/api/admin/overview").await.body["backup_overdue"], true);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_audit_log_filters_and_names_admins(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, _) = licensing.issue(json!({})).await;
    licensing.admin_post(&format!("/api/admin/licences/{licence}/revoke"), json!({})).await;

    let all = licensing.admin.get("/api/admin/audit?limit=100").await.body;
    let actions: Vec<&str> =
        all["data"].as_array().unwrap().iter().map(|entry| entry["action"].as_str().unwrap()).collect();
    assert_eq!(&actions[..3], ["licence.revoked", "licence.issued", "product.created"], "newest first");
    assert_eq!(all["data"][0]["ip_address"], "198.51.100.10");

    let licence_entries = licensing.admin.get("/api/admin/audit?action=licence.").await.body;
    assert_eq!(licence_entries["data"].as_array().unwrap().len(), 2);
    let one = licensing
        .admin
        .get(&format!("/api/admin/audit?entity={licence}&action=licence.revoked&expand=admin_user"))
        .await
        .body;
    assert_eq!(one["data"][0]["admin_user"]["username"], "ran");
    let none = licensing
        .admin
        .get(&format!(
            "/api/admin/audit?from={}",
            urlencode(&(licensing.now() + chrono::Duration::hours(1)).to_rfc3339())
        ))
        .await
        .body;
    assert!(none["data"].as_array().unwrap().is_empty());
    assert_eq!(licensing.admin.get("/api/admin/audit?from=yesterday").await.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn preferences_are_saved_restored_and_checked(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let defaults = licensing.admin.get("/api/admin/preferences").await.body;
    assert_eq!(defaults, json!({ "version": 1, "theme": "system", "tables": {} }));

    let chosen = json!({
        "version": 1,
        "theme": "dark",
        "last_page": "/licences",
        "tables": { "licences": { "filters": { "state": "unused" }, "search": "López", "order": "expiry", "page_size": 25, "columns": ["key", "customer"] } }
    });
    let saved = licensing.admin.call(Method::PUT, "/api/admin/preferences", Some(chosen.clone())).await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.body);
    assert_eq!(licensing.admin.get("/api/admin/preferences").await.body, chosen, "restored on the next visit");

    for bad in [
        json!({ "version": 1, "theme": "neon" }),
        json!({ "version": 1, "tables": { "licences": { "search": "BRN-79YN-44XQ-W025-377Z" } } }),
        json!({ "version": 1, "tables": { "licences": { "page_size": 5 } } }),
        json!({ "version": 3 }),
        json!({ "version": 1, "colour": "red" }),
    ] {
        let reply = licensing.admin.call(Method::PUT, "/api/admin/preferences", Some(bad.clone())).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{bad}");
    }
    assert_eq!(licensing.admin.get("/api/admin/preferences").await.body, chosen, "nothing invalid was saved");
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
