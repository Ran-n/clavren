// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:17.167017
//+ Revised: 	2026/11/03 04:54:17.167017
// -----------------------------------------------------------------------------

//! A licence after issuing: editing, revoking, resetting (one at a time and in bulk, all or
//! nothing), and the ways its key is shown again: reveal, message and CSV export.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use axum::http::{Method, StatusCode, header};
use clavren::clock::Clock;
use common::api::{Admin, Reply};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

struct Licences {
    admin: Admin,
    product: String,
}

impl Licences {
    async fn new(pool: PgPool) -> Self {
        let admin = Admin::new(pool).await;
        let body = json!({ "code": "avalio-lite", "name": "Avalio Lite", "message_template": "Dear {customer_name}: {key} ({product_name})" });
        let product = admin.create("/api/admin/products", body).await["id"].as_str().unwrap().to_string();
        Self { admin, product }
    }

    /// Issues `quantity` licences; returns `(id, key)` of each.
    async fn issue(&self, quantity: usize, customer: Value) -> Vec<(String, String)> {
        let body = json!({ "product": self.product, "quantity": quantity, "customer": customer });
        let issued = self.admin.create("/api/admin/licences", body).await;
        issued["licences"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| (l["id"].as_str().unwrap().into(), l["key"].as_str().unwrap().into()))
            .collect()
    }

    async fn post(&self, path: &str, body: Value) -> Reply {
        self.admin.call(Method::POST, &format!("/api/admin/licences{path}"), Some(body)).await
    }

    async fn get(&self, id: &str) -> Value {
        self.admin.get(&format!("/api/admin/licences/{id}?expand=active_activation")).await.body
    }

    async fn activate(&self, id: &str) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO activations (license_id, device_public_key, device_name, platform, app_version)
             VALUES ($1::uuid, $2, 'Ana''s PC', 'windows', '1.0.0') RETURNING id",
        )
        .bind(id)
        .bind([9u8; 32].as_slice())
        .fetch_one(self.admin.pool())
        .await
        .unwrap()
    }

    async fn audited(&self, action: &str) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = $1")
            .bind(action)
            .fetch_one(self.admin.pool())
            .await
            .unwrap()
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn expiry_and_notes_are_edited_in_place(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let id = licences.issue(1, Value::Null).await.remove(0).0;
    let path = format!("/api/admin/licences/{id}");
    let end = (licences.admin.fixture.clock.now() + chrono::Duration::days(365)).to_rfc3339();

    let reply =
        licences.admin.call(Method::PATCH, &path, Some(json!({ "expires_at": end, "internal_notes": " Paid " }))).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["internal_notes"], "Paid");
    assert!(reply.body["expires_at"].is_string());
    let reply = licences.admin.call(Method::PATCH, &path, Some(json!({ "expires_at": null }))).await;
    assert_eq!(
        (reply.body["expires_at"].clone(), reply.body["internal_notes"].as_str()),
        (Value::Null, Some("Paid")),
        "permanent again"
    );
    let expanded = licences
        .admin
        .call(Method::PATCH, &format!("{path}?expand=product"), Some(json!({ "internal_notes": "Paid in full" })))
        .await;
    assert_eq!(expanded.body["product"]["code"], "avalio-lite", "the answer expands like a read");
    let wrong = licences
        .admin
        .call(Method::PATCH, &format!("{path}?expand=owner"), Some(json!({ "internal_notes": "x" })))
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST, "checked before anything changes");

    let reply = licences.admin.call(Method::PATCH, &path, Some(json!({}))).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"));
    let reply = licences
        .admin
        .call(Method::PATCH, &format!("/api/admin/licences/{}", Uuid::now_v7()), Some(json!({ "internal_notes": "x" })))
        .await;
    assert_eq!((reply.status, reply.body.get("ids")), (StatusCode::NOT_FOUND, None));
    assert_eq!(licences.audited("licence.updated").await, 3);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn bulk_edits_change_all_or_nothing(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let ids: Vec<String> = licences.issue(3, Value::Null).await.into_iter().map(|(id, _)| id).collect();
    let ghost = Uuid::now_v7().to_string();

    let reply = licences.post("/bulk/edit", json!({ "ids": [ids[0], ghost, ids[1]], "internal_notes": "Batch" })).await;
    assert_eq!((reply.status, reply.body["ids"].clone()), (StatusCode::NOT_FOUND, json!([ghost])));
    assert_eq!(licences.get(&ids[0]).await["internal_notes"], Value::Null, "nothing changed");

    let reply = licences.post("/bulk/edit", json!({ "ids": ids, "internal_notes": "Batch" })).await;
    assert_eq!((reply.status, reply.body["updated"].clone()), (StatusCode::OK, json!(3)));
    for id in &ids {
        assert_eq!(licences.get(id).await["internal_notes"], "Batch");
    }
    assert_eq!(licences.audited("licence.updated").await, 3, "one entry per licence");

    let duplicate = licences.post("/bulk/edit", json!({ "ids": [ids[0], ids[0]], "internal_notes": "x" })).await;
    assert_eq!(duplicate.status, StatusCode::BAD_REQUEST);
    let too_many: Vec<String> = (0..1001).map(|_| Uuid::now_v7().to_string()).collect();
    assert_eq!(
        licences.post("/bulk/edit", json!({ "ids": too_many, "internal_notes": "x" })).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        licences.post("/bulk/edit", json!({ "ids": [], "internal_notes": "x" })).await.status,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn revoking_deletes_the_key_and_ends_the_activation(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let id = licences.issue(1, Value::Null).await.remove(0).0;
    let activation = licences.activate(&id).await;
    assert_eq!(licences.get(&id).await["state"], "redeemed");

    let reply = licences.post(&format!("/{id}/revoke"), json!({ "reason": "Refunded" })).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(
        (reply.body["state"].as_str(), reply.body["revocation_reason"].as_str()),
        (Some("revoked"), Some("Refunded"))
    );

    let secrets: i64 =
        sqlx::query_scalar("SELECT count(*) FROM license_key_secrets").fetch_one(licences.admin.pool()).await.unwrap();
    assert_eq!(secrets, 0, "the encrypted key is gone");
    let lookup: i64 = sqlx::query_scalar("SELECT count(*) FROM licenses WHERE key_lookup IS NOT NULL")
        .fetch_one(licences.admin.pool())
        .await
        .unwrap();
    assert_eq!(lookup, 1, "the hash stays, so the key is never accepted again");
    let reason: String = sqlx::query_scalar("SELECT revocation_reason FROM activations WHERE id = $1")
        .bind(activation)
        .fetch_one(licences.admin.pool())
        .await
        .unwrap();
    assert_eq!(reason, "licence_revoked");

    let again = licences.post(&format!("/{id}/revoke"), json!({})).await;
    assert_eq!((again.status, again.error()), (StatusCode::CONFLICT, "licence_revoked"));
    for action in ["/reveal", "/message", "/reset"] {
        let reply = licences.post(&format!("/{id}{action}"), json!({})).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "licence_revoked"), "{action}");
    }
    assert_eq!(licences.audited("licence.revoked").await, 1);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_bulk_revoke_with_an_already_revoked_licence_changes_nothing(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let ids: Vec<String> = licences.issue(3, Value::Null).await.into_iter().map(|(id, _)| id).collect();
    licences.post(&format!("/{}/revoke", ids[2]), json!({})).await;

    let reply = licences.post("/bulk/revoke", json!({ "ids": ids })).await;
    assert_eq!(
        (reply.status, reply.error(), reply.body["ids"].clone()),
        (StatusCode::CONFLICT, "licence_revoked", json!([ids[2]]))
    );
    assert_eq!(licences.get(&ids[0]).await["state"], "unused");

    let reply = licences.post("/bulk/revoke", json!({ "ids": &ids[..2], "reason": "Chargeback" })).await;
    assert_eq!((reply.status, reply.body["revoked"].clone()), (StatusCode::OK, json!(2)));
    assert_eq!(licences.get(&ids[1]).await["revocation_reason"], "Chargeback");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn reset_frees_a_licence_in_use(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let id = licences.issue(1, Value::Null).await.remove(0).0;
    let unused = licences.post(&format!("/{id}/reset"), json!({})).await;
    assert_eq!((unused.status, unused.error()), (StatusCode::CONFLICT, "licence_not_in_use"));

    let activation = licences.activate(&id).await;
    let in_use = licences.get(&id).await;
    assert_eq!(in_use["active_activation"]["id"], activation.to_string());
    assert_eq!(in_use["active_activation"]["device_name"], "Ana's PC");

    let reply = licences.post(&format!("/{id}/reset"), json!({})).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!((reply.body["state"].as_str(), reply.body["active_activation"].clone()), (Some("unused"), Value::Null));
    let reason: String = sqlx::query_scalar("SELECT revocation_reason FROM activations WHERE id = $1")
        .bind(activation)
        .fetch_one(licences.admin.pool())
        .await
        .unwrap();
    assert_eq!(reason, "reset");
    licences.activate(&id).await;
    assert_eq!(licences.get(&id).await["state"], "redeemed", "the same key works on the next computer");
    assert_eq!(licences.post(&format!("/{}/reset", Uuid::now_v7()), json!({})).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn reveal_and_message_show_the_key_and_are_audited(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let (id, key) = licences.issue(1, json!({ "name": "Ana" })).await.remove(0);
    let (bare, bare_key) = licences.issue(1, Value::Null).await.remove(0);

    let revealed = licences.post(&format!("/{id}/reveal"), json!({})).await;
    assert_eq!(revealed.body["key"], key.as_str());
    let message = licences.post(&format!("/{id}/message"), json!({})).await;
    assert_eq!(message.body["message"], format!("Dear Ana: {key} (Avalio Lite)"));
    let message = licences.post(&format!("/{bare}/message"), json!({})).await;
    assert_eq!(message.body["message"], format!("Dear : {bare_key} (Avalio Lite)"));

    assert_eq!(
        (licences.audited("licence.key_revealed").await, licences.audited("licence.message_copied").await),
        (1, 2)
    );
    let unknown = licences.post(&format!("/{}/reveal", Uuid::now_v7()), json!({})).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_export_is_safe_csv_with_the_keys_still_held(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let (ana, ana_key) = licences.issue(1, json!({ "name": "López, Ana", "email": "ana@example.com" })).await.remove(0);
    let (revoked, _) = licences.issue(1, Value::Null).await.remove(0);
    licences
        .admin
        .call(
            Method::PATCH,
            &format!("/api/admin/licences/{ana}"),
            Some(json!({ "internal_notes": "=HYPERLINK(\"x\")" })),
        )
        .await;
    licences.post(&format!("/{revoked}/revoke"), json!({})).await;

    let reply = licences.post("/export", json!({ "ids": [ana, revoked] })).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    assert_eq!(reply.headers[header::CONTENT_TYPE], "text/csv; charset=utf-8");
    assert!(
        reply.headers[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment; filename=\"clavren-licences-")
    );
    let lines: Vec<&str> = reply.text.split("\r\n").collect();
    assert_eq!(lines[0], "licence_id,product_code,customer_name,customer_email,key,state,issued_at,expires_at,notes");
    let ana_line = lines.iter().find(|line| line.starts_with(&ana)).unwrap();
    assert!(ana_line.contains(&format!(",avalio-lite,\"López, Ana\",ana@example.com,{ana_key},unused,")), "{ana_line}");
    assert!(ana_line.ends_with(",\"'=HYPERLINK(\"\"x\"\")\""), "{ana_line}");
    let revoked_line = lines.iter().find(|line| line.starts_with(&revoked)).unwrap();
    assert!(revoked_line.contains(",,,,revoked,"), "no key left: {revoked_line}");
    assert_eq!(licences.audited("licence.exported").await, 2);

    let by_filter = licences.post("/export", json!({ "filter": { "state": "unused" } })).await;
    assert_eq!(by_filter.text.split("\r\n").filter(|line| !line.is_empty()).count(), 2, "header and Ana");
    for body in [
        json!({}),
        json!({ "ids": [ana], "filter": {} }),
        json!({ "ids": [] }),
        json!({ "filter": { "state": "lost" } }),
    ] {
        assert_eq!(licences.post("/export", body.clone()).await.status, StatusCode::BAD_REQUEST, "{body}");
    }
    let missing = licences.post("/export", json!({ "ids": [Uuid::now_v7()] })).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}
