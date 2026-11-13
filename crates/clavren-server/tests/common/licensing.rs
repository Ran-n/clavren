// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.513158
//+ Revised: 	2026/11/11 16:32:51.817913
// -----------------------------------------------------------------------------

//! A server with a product and an admin to issue keys for it: the starting point of every test
//! of the public API.

use axum::Router;
use axum::http::Method;
use clavren::clock::Clock;
use serde_json::{Value, json};
use sqlx::PgPool;

use super::api::{Admin, Reply};

pub const PRODUCT: &str = "avalio-lite";

pub struct Licensing {
    pub admin: Admin,
    pub product_id: String,
}

impl Licensing {
    pub async fn new(pool: PgPool) -> Self {
        Self::with_settings(pool, super::test_settings()).await
    }

    pub async fn with_settings(pool: PgPool, settings: clavren::http::Settings) -> Self {
        let admin = Admin::with_settings(pool, settings).await;
        let product = admin
            .create("/api/admin/products", json!({ "code": PRODUCT, "name": "Avalio Lite", "test_licence": false }))
            .await;
        Self { admin, product_id: product["id"].as_str().unwrap().to_string() }
    }

    pub fn app(&self) -> &Router {
        &self.admin.fixture.app
    }

    pub fn now(&self) -> chrono::DateTime<chrono::Utc> {
        self.admin.fixture.clock.now()
    }

    /// Issues one licence with the extra fields in `extra`; returns its ID and key.
    pub async fn issue(&self, extra: Value) -> (String, String) {
        let mut body = json!({ "product": self.product_id });
        body.as_object_mut().unwrap().extend(extra.as_object().cloned().unwrap_or_default());
        let issued = self.admin.create("/api/admin/licences", body).await;
        let licence = &issued["licences"][0];
        (licence["id"].as_str().unwrap().to_string(), licence["key"].as_str().unwrap().to_string())
    }

    pub async fn admin_post(&self, path: &str, body: Value) -> Reply {
        self.admin.call(Method::POST, path, Some(body)).await
    }

    pub async fn licence(&self, id: &str) -> Value {
        self.admin.get(&format!("/api/admin/licences/{id}?expand=active_activation")).await.body
    }
}
