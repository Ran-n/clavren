// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.519572
//+ Revised: 	2026/11/03 04:54:23.045297
// -----------------------------------------------------------------------------

//! Calling the HTTP API in-process, as a browser would: JSON bodies, the session cookie, and the
//! CSRF header.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use clavren::clock::ManualClock;
use clavren::http::AppState;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

use super::admin::{PASSWORD, SeededAdmin, seed_admin};
use super::{test_app, test_state};

pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
    /// The body as text, for answers that aren't JSON.
    pub text: String,
}

impl Reply {
    /// The session token from `Set-Cookie`, if one was set.
    pub fn session_cookie(&self) -> Option<String> {
        let set_cookie = self.headers.get(header::SET_COOKIE)?.to_str().ok()?;
        let value = set_cookie.strip_prefix("clavren_session=")?.split(';').next()?;
        (!value.is_empty()).then(|| value.to_string())
    }

    pub fn error(&self) -> &str {
        self.body["error"].as_str().unwrap_or_default()
    }
}

/// A signed-in browser: its cookie and CSRF token.
#[derive(Clone)]
pub struct Browser {
    pub cookie: String,
    pub csrf: String,
}

pub struct Call<'a> {
    pub method: Method,
    pub path: &'a str,
    pub body: Option<Value>,
    pub browser: Option<&'a Browser>,
    pub csrf_override: Option<&'a str>,
    pub forwarded_for: Option<&'a str>,
}

impl<'a> Call<'a> {
    pub fn new(method: Method, path: &'a str) -> Self {
        Self { method, path, body: None, browser: None, csrf_override: None, forwarded_for: None }
    }

    pub fn json(self, body: Value) -> Self {
        Self { body: Some(body), ..self }
    }

    pub fn signed_in(self, browser: &'a Browser) -> Self {
        Self { browser: Some(browser), ..self }
    }

    pub fn csrf(self, token: &'a str) -> Self {
        Self { csrf_override: Some(token), ..self }
    }

    pub fn forwarded_for(self, address: &'a str) -> Self {
        Self { forwarded_for: Some(address), ..self }
    }

    pub async fn send(self, app: &Router) -> Reply {
        let mut request = Request::builder().method(self.method).uri(self.path);
        if let Some(browser) = self.browser {
            request = request.header(header::COOKIE, format!("theme=dark; clavren_session={}", browser.cookie));
        }
        if let Some(csrf) = self.csrf_override.or(self.browser.map(|browser| browser.csrf.as_str())) {
            request = request.header("x-csrf-token", csrf);
        }
        if let Some(address) = self.forwarded_for {
            request = request.header("cf-connecting-ip", address);
        }
        let body = match self.body {
            Some(json) => {
                request = request.header(header::CONTENT_TYPE, "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        let response = app.clone().oneshot(request.body(body).unwrap()).await.unwrap();
        let (status, headers) = (response.status(), response.headers().clone());
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Reply { status, headers, body, text: String::from_utf8_lossy(&bytes).into_owned() }
    }
}

pub fn client() -> SocketAddr {
    "198.51.100.10:50000".parse().unwrap()
}

pub async fn login(app: &Router, username: &str, password: &str, code: &str) -> Reply {
    Call::new(Method::POST, "/api/admin/session")
        .json(serde_json::json!({ "username": username, "password": password, "code": code }))
        .send(app)
        .await
}

pub async fn sign_in(app: &Router, username: &str, password: &str, code: &str) -> Browser {
    let reply = login(app, username, password, code).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    Browser { cookie: reply.session_cookie().unwrap(), csrf: reply.body["csrf_token"].as_str().unwrap().to_string() }
}

/// A server with one admin, `ran`, reached from [`client`].
pub struct Fixture {
    pub app: Router,
    pub state: AppState,
    pub clock: Arc<ManualClock>,
    pub admin: SeededAdmin,
}

impl Fixture {
    pub async fn new(pool: PgPool) -> Self {
        let (state, clock) = test_state(pool.clone());
        let admin = seed_admin(&pool, &clock, "ran").await;
        Self { app: test_app(state.clone(), client()), state, clock, admin }
    }

    pub fn code(&self) -> String {
        self.admin.code_now(&self.clock)
    }

    /// Signs in with the current code, then moves on to the next TOTP step so the next sign-in
    /// has a fresh code.
    pub async fn sign_in(&self) -> Browser {
        let browser = sign_in(&self.app, "ran", PASSWORD, &self.code()).await;
        self.clock.advance(Duration::from_secs(30));
        browser
    }

    pub fn pool(&self) -> &PgPool {
        &self.state.pool
    }
}

/// A signed-in admin of a [`Fixture`], calling the admin API.
pub struct Admin {
    pub fixture: Fixture,
    pub browser: Browser,
}

impl Admin {
    pub async fn new(pool: PgPool) -> Self {
        let fixture = Fixture::new(pool).await;
        let browser = fixture.sign_in().await;
        Self { fixture, browser }
    }

    pub async fn call(&self, method: Method, path: &str, body: Option<Value>) -> Reply {
        let call = Call::new(method, path).signed_in(&self.browser);
        match body {
            Some(body) => call.json(body).send(&self.fixture.app).await,
            None => call.send(&self.fixture.app).await,
        }
    }

    pub async fn get(&self, path: &str) -> Reply {
        self.call(Method::GET, path, None).await
    }

    /// `POST`s `body` and expects `201 Created`.
    pub async fn create(&self, path: &str, body: Value) -> Value {
        let reply = self.call(Method::POST, path, Some(body)).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
        reply.body
    }

    pub fn pool(&self) -> &PgPool {
        self.fixture.pool()
    }

    /// Moves the clock on, then signs in again: a long jump ends the old session.
    pub async fn wait(&mut self, time: Duration) {
        self.fixture.clock.advance(time);
        self.browser = self.fixture.sign_in().await;
    }
}
