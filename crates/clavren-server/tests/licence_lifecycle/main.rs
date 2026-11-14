// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:17.167017
//+ Revised: 	2026/11/14 12:05:26.339585
// -----------------------------------------------------------------------------

//! A licence after issuing: editing every field but the product (customer, expiry, notes, the
//! revocation reason), revoking, resetting, deleting (one at a time and in bulk, all or nothing),
//! and the ways its key is shown again: reveal, message and CSV export.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod bulk_and_customers;
#[path = "../common/mod.rs"]
mod common;
mod deleting;
mod disclosure;
mod editing;

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
        let body = json!({ "code": "avalio-lite", "name": "Avalio Lite", "message_template": "Dear {customer_name}: {key} ({product_name})", "test_licence": false });
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

    async fn patch(&self, id: &str, body: Value) -> Reply {
        self.admin.call(Method::PATCH, &format!("/api/admin/licences/{id}?expand=customer"), Some(body)).await
    }

    async fn customer(&self, name: &str, email: &str) -> String {
        let customer = self.admin.create("/api/admin/customers", json!({ "name": name, "email": email })).await;
        customer["id"].as_str().unwrap().to_string()
    }

    async fn customers(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM customers").fetch_one(self.admin.pool()).await.unwrap()
    }

    /// Every audit entry's metadata, as text.
    async fn audit_metadata(&self) -> String {
        let rows: Vec<Value> =
            sqlx::query_scalar("SELECT metadata FROM audit_logs").fetch_all(self.admin.pool()).await.unwrap();
        serde_json::to_string(&rows).unwrap()
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
