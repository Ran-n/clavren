// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.580530
//+ Revised: 	2026/11/03 04:54:23.173912
// -----------------------------------------------------------------------------

//! A signed-in admin changing their own password, recovery codes and authenticator app.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use clavren::admin_auth::totp;
use clavren::clock::Clock;
use common::admin::PASSWORD;
use common::api::{Browser, Call, Fixture, Reply, login};
use serde_json::{Value, json};
use sqlx::PgPool;

const NEW_PASSWORD: &str = "an even better passphrase";

async fn post(fixture: &Fixture, browser: &Browser, path: &str, body: Value) -> Reply {
    Call::new(Method::POST, path).signed_in(browser).json(body).send(&fixture.app).await
}

async fn change_password(fixture: &Fixture, browser: &Browser, current: &str, new: &str) -> Reply {
    post(fixture, browser, "/api/admin/account/password", json!({ "current_password": current, "new_password": new }))
        .await
}

async fn audited(fixture: &Fixture, action: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = $1")
        .bind(action)
        .fetch_one(fixture.pool())
        .await
        .unwrap()
}

async fn whoami(fixture: &Fixture, browser: &Browser) -> StatusCode {
    Call::new(Method::GET, "/api/admin/session").signed_in(browser).send(&fixture.app).await.status
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_new_password_replaces_the_old_and_signs_out_other_sessions(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let laptop = fixture.sign_in().await;
    let phone = fixture.sign_in().await;

    let reply = change_password(&fixture, &laptop, PASSWORD, NEW_PASSWORD).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);
    assert_eq!(whoami(&fixture, &laptop).await, StatusCode::OK);
    assert_eq!(whoami(&fixture, &phone).await, StatusCode::UNAUTHORIZED);

    assert_eq!(login(&fixture.app, "ran", PASSWORD, &fixture.code()).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(login(&fixture.app, "ran", NEW_PASSWORD, &fixture.code()).await.status, StatusCode::OK);
    assert_eq!(audited(&fixture, "admin.password_changed").await, 1);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_weak_new_password_is_refused_and_nothing_changes(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    let reply = change_password(&fixture, &browser, PASSWORD, "too short").await;
    assert_eq!((reply.status, reply.error()), (StatusCode::UNPROCESSABLE_ENTITY, "weak_password"));
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &fixture.code()).await.status, StatusCode::OK);
}

/// A stolen session must not become a way to guess the password: wrong guesses count towards
/// the same limit as failed logins.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn wrong_current_passwords_count_towards_the_login_limit(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    for _ in 0..5 {
        let reply = change_password(&fixture, &browser, "a guessed password", NEW_PASSWORD).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::UNAUTHORIZED, "invalid_credentials"));
    }
    let reply = change_password(&fixture, &browser, PASSWORD, NEW_PASSWORD).await;
    assert_eq!(reply.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &fixture.code()).await.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(whoami(&fixture, &browser).await, StatusCode::OK, "the session itself stays valid");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn account_changes_need_the_csrf_token(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    let body = json!({ "current_password": PASSWORD, "new_password": NEW_PASSWORD });
    let reply = Call::new(Method::POST, "/api/admin/account/password")
        .signed_in(&browser)
        .csrf("")
        .json(body)
        .send(&fixture.app)
        .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    let reply =
        Call::new(Method::POST, "/api/admin/account/totp/secret").signed_in(&browser).csrf("").send(&fixture.app).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn new_recovery_codes_replace_the_old_ones(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    let reply = post(
        &fixture,
        &browser,
        "/api/admin/account/recovery-codes",
        json!({ "password": PASSWORD, "code": fixture.code() }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    let codes: Vec<String> = serde_json::from_value(reply.body["recovery_codes"].clone()).unwrap();
    assert_eq!(codes.len(), 10);
    assert!(codes.iter().all(|code| !fixture.admin.recovery_codes.contains(code)));

    assert_eq!(
        login(&fixture.app, "ran", PASSWORD, &fixture.admin.recovery_codes[0]).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &codes[0]).await.status, StatusCode::OK);
    assert_eq!(audited(&fixture, "admin.recovery_codes_regenerated").await, 1);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn new_recovery_codes_need_the_second_factor_too(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    let reply = post(
        &fixture,
        &browser,
        "/api/admin/account/recovery-codes",
        json!({ "password": PASSWORD, "code": "000000" }),
    )
    .await;
    assert_eq!((reply.status, reply.error()), (StatusCode::UNAUTHORIZED, "invalid_credentials"));
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &fixture.admin.recovery_codes[0]).await.status, StatusCode::OK);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_new_authenticator_replaces_the_old_one_once_confirmed(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;

    let reply = post(&fixture, &browser, "/api/admin/account/totp/secret", Value::Null).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    let secret_text = reply.body["secret"].as_str().unwrap().to_string();
    let uri = reply.body["uri"].as_str().unwrap();
    assert!(uri.starts_with("otpauth://totp/") && uri.contains(&secret_text) && uri.contains("ran"), "{uri}");
    let qr = reply.body["qr_svg"].as_str().unwrap();
    assert!(qr.starts_with("<?xml") && qr.contains("<svg"), "the dashboard shows the QR code as an image: {qr}");
    assert_eq!(
        login(&fixture.app, "ran", PASSWORD, &fixture.code()).await.status,
        StatusCode::OK,
        "nothing stored yet"
    );
    fixture.clock.advance(Duration::from_secs(30));

    let secret = totp::secret_from_text(&secret_text).unwrap();
    let new_code = totp::code_at(&secret, "ran", fixture.clock.now()).unwrap();
    let body = json!({ "password": PASSWORD, "secret": secret_text, "code": new_code });
    let reply =
        Call::new(Method::PUT, "/api/admin/account/totp").signed_in(&browser).json(body).send(&fixture.app).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);
    assert_eq!(audited(&fixture, "admin.totp_changed").await, 1);

    assert_eq!(
        login(&fixture.app, "ran", PASSWORD, &new_code).await.status,
        StatusCode::UNAUTHORIZED,
        "the confirming code is spent"
    );
    fixture.clock.advance(Duration::from_secs(30));
    assert_eq!(
        login(&fixture.app, "ran", PASSWORD, &fixture.code()).await.status,
        StatusCode::UNAUTHORIZED,
        "the old app"
    );
    let next_code = totp::code_at(&secret, "ran", fixture.clock.now()).unwrap();
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &next_code).await.status, StatusCode::OK);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_new_authenticator_needs_a_valid_secret_and_matching_code(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    let secret_text = post(&fixture, &browser, "/api/admin/account/totp/secret", Value::Null).await.body["secret"]
        .as_str()
        .unwrap()
        .to_string();
    let replace = |secret: String, code: String, password: &'static str| {
        let body = json!({ "password": password, "secret": secret, "code": code });
        Call::new(Method::PUT, "/api/admin/account/totp").signed_in(&browser).json(body).send(&fixture.app)
    };

    let reply = replace("not base32!".into(), "123456".into(), PASSWORD).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"));
    let reply = replace(secret_text.clone(), fixture.code(), PASSWORD).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "a code from the old app");
    let reply = replace(secret_text, "123456".into(), "a wrong password").await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);

    assert_eq!(audited(&fixture, "admin.totp_changed").await, 0);
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &fixture.code()).await.status, StatusCode::OK);
}
