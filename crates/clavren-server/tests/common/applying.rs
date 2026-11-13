// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:49.346002
//+ Revised: 	2026/11/11 16:32:54.038893
// -----------------------------------------------------------------------------

//! Licence applications in tests: a product that takes them, sending one, asking how it went, and
//! a stand-in for Cloudflare's Turnstile check.

use std::net::SocketAddr;

use axum::http::{Method, StatusCode};
use axum::routing::post;
use axum::{Form, Json, Router};
use clavren::config::TurnstileSettings;
use clavren::http::Settings;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use super::api::{Admin, Call, Reply};
use super::licensing::Licensing;
use super::test_settings;

/// The Turnstile answer the stand-in accepts.
pub const GOOD_ANSWER: &str = "good-answer";
/// The secret the stand-in takes; any other is refused as Cloudflare refuses a wrong one.
pub const TURNSTILE_SECRET: &str = "turnstile-secret";

pub struct Applying {
    pub licensing: Licensing,
}

impl Applying {
    /// The [`Licensing`] product, taking applications.
    pub async fn new(pool: PgPool) -> Self {
        Self::with_settings(pool, test_settings()).await
    }

    pub async fn with_settings(pool: PgPool, settings: Settings) -> Self {
        let licensing = Licensing::with_settings(pool, settings).await;
        let path = format!("/api/admin/products/{}", licensing.product_id);
        let opened = licensing.admin.call(Method::PATCH, &path, Some(json!({ "accepts_applications": true }))).await;
        assert_eq!(opened.status, StatusCode::OK, "{:?}", opened.body);
        Self { licensing }
    }

    pub fn admin(&self) -> &Admin {
        &self.licensing.admin
    }

    /// Sends an application from Ana for the product, with the fields in `extra` on top.
    pub async fn apply(&self, extra: Value) -> Reply {
        let mut body = json!({ "product": super::licensing::PRODUCT, "name": "Ana López", "email": "ana@example.com" });
        body.as_object_mut().unwrap().extend(extra.as_object().cloned().unwrap_or_default());
        Call::new(Method::POST, "/api/v1/applications").json(body).send(self.licensing.app()).await
    }

    /// [`Self::apply`], expecting it taken; returns the application's ID and secret.
    pub async fn applied(&self, extra: Value) -> (String, String) {
        let reply = self.apply(extra).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
        (reply.body["application"].as_str().unwrap().into(), reply.body["secret"].as_str().unwrap().into())
    }

    pub async fn status(&self, secret: &str) -> Reply {
        Call::new(Method::POST, "/api/v1/applications/status")
            .json(json!({ "secret": secret }))
            .send(self.licensing.app())
            .await
    }

    pub async fn approve(&self, id: &str, body: Value) -> Reply {
        self.licensing.admin_post(&format!("/api/admin/applications/{id}/approve?expand=licence.customer"), body).await
    }

    pub async fn reject(&self, id: &str, body: Value) -> Reply {
        self.licensing.admin_post(&format!("/api/admin/applications/{id}/reject"), body).await
    }

    pub async fn application(&self, id: &str) -> Value {
        self.admin().get(&format!("/api/admin/applications/{id}")).await.body
    }
}

#[derive(Deserialize)]
struct Asked {
    secret: String,
    response: String,
    remoteip: String,
}

/// Starts a stand-in for Turnstile's `siteverify`: it accepts [`GOOD_ANSWER`] from 198.51.100.10
/// (the tests' client) when the secret is [`TURNSTILE_SECRET`]. Returns settings that use it.
pub async fn fake_turnstile() -> Settings {
    async fn verify(Form(asked): Form<Asked>) -> Json<Value> {
        if asked.secret != TURNSTILE_SECRET {
            return Json(json!({ "success": false, "error-codes": ["invalid-input-secret"] }));
        }
        let success = asked.response == GOOD_ANSWER && asked.remoteip == "198.51.100.10";
        Json(
            json!({ "success": success, "error-codes": if success { json!([]) } else { json!(["invalid-input-response"]) } }),
        )
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, Router::new().route("/siteverify", post(verify))).await });
    turnstile_settings(&format!("http://{address}/siteverify"), TURNSTILE_SECRET)
}

pub fn turnstile_settings(verify_url: &str, secret: &str) -> Settings {
    let mut settings = test_settings();
    settings.applications.turnstile =
        Some(TurnstileSettings { site_key: "site-key".into(), secret: secret.into(), verify_url: verify_url.into() });
    settings
}
