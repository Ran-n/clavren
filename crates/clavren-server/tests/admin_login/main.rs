// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.698814
//+ Revised: 	2026/11/14 12:05:24.483220
// -----------------------------------------------------------------------------

//! `POST /api/admin/session`: signing in with password and second factor, the single answer for
//! every failure, one-time codes, rate limits and client addresses behind the tunnel.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

#[path = "../common/mod.rs"]
mod common;
mod limits;

use std::sync::Arc;
use std::time::Duration;

use axum::http::{Method, StatusCode, header};
use clavren::admin_cli::Command;
use clavren::clock::{Clock, ManualClock};
use common::admin::{PASSWORD, run_cli, seed_admin};
use common::api::{Call, Fixture, client, login};
use common::{capture_logs, test_app, test_start};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn audit_rows(pool: &PgPool) -> Vec<(String, Option<String>, Value)> {
    sqlx::query_as("SELECT action, host(ip_address), metadata FROM audit_logs WHERE action LIKE 'admin.login%' ORDER BY created_at")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_correct_login_opens_a_session(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let reply = login(&fixture.app, "ran", PASSWORD, &fixture.code()).await;

    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");
    let cookie = reply.headers[header::SET_COOKIE].to_str().unwrap();
    for attribute in ["HttpOnly", "SameSite=Strict", "Secure", "Path=/", "Max-Age=604800"] {
        assert!(cookie.contains(attribute), "{attribute} missing from {cookie}");
    }
    assert_eq!(reply.body["admin"]["username"], "ran");
    assert_eq!(reply.body["admin"]["id"], fixture.admin.id.to_string());
    assert_eq!(reply.body["csrf_token"].as_str().unwrap().len(), 43);
    assert!(reply.body["expires_at"].is_string());

    let token = reply.session_cookie().unwrap();
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT token_hash FROM admin_sessions").fetch_one(fixture.pool()).await.unwrap();
    assert_eq!(stored, blake3::hash(token.as_bytes()).as_bytes(), "only the token's hash is stored");
    let last_login: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT last_login_at FROM admin_users").fetch_one(fixture.pool()).await.unwrap();
    assert_eq!(last_login, Some(fixture.clock.now()));
    assert_eq!(audit_rows(fixture.pool()).await, [("admin.login".into(), Some("198.51.100.10".into()), json!({}))]);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_username_ignores_case_and_surrounding_spaces(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let reply = login(&fixture.app, "  RAN ", PASSWORD, &fixture.code()).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["admin"]["username"], "ran");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn every_failure_gets_the_same_answer_and_is_audited_with_its_reason(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    run_cli(
        fixture.pool(),
        &fixture.clock,
        Command::CreateAdmin { username: "off".into() },
        "off",
        common::admin::smooth_enrolment(PASSWORD),
    )
    .await
    .0
    .unwrap();
    run_cli(fixture.pool(), &fixture.clock, Command::DisableAdmin { username: "off".into() }, "off", vec![])
        .await
        .0
        .unwrap();
    fixture.clock.advance(Duration::from_secs(60));

    let attempts = [
        ("ran", "wrong password entirely", fixture.code()),
        ("ran", PASSWORD, "000000".to_string()),
        ("ran", PASSWORD, "AAAAA-AAAAA-AAAAA-AAAAA-AAAAA-A".to_string()),
        ("nobody", PASSWORD, fixture.code()),
        ("off", PASSWORD, "123456".to_string()),
    ];
    let mut bodies = Vec::new();
    for (username, password, code) in &attempts {
        let reply = login(&fixture.app, username, password, code).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
        assert!(reply.session_cookie().is_none());
        bodies.push(reply.body);
    }
    assert!(bodies.iter().all(|body| *body == bodies[0]), "{bodies:?}");
    assert_eq!(bodies[0]["error"], "invalid_credentials");

    let reasons: Vec<Value> =
        audit_rows(fixture.pool()).await.into_iter().map(|(_, _, metadata)| metadata["reason"].clone()).collect();
    assert_eq!(reasons, ["password", "second_factor", "second_factor", "unknown_user", "disabled"]);
    let session_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM admin_sessions").fetch_one(fixture.pool()).await.unwrap();
    assert_eq!(session_count, 0);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_authenticator_code_works_once(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let code = fixture.code();
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &code).await.status, StatusCode::OK);
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &code).await.status, StatusCode::UNAUTHORIZED);

    fixture.clock.advance(Duration::from_secs(30));
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &fixture.code()).await.status, StatusCode::OK);
}

/// The code typed during `create-admin` must not open a session, even within its 30 seconds.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_code_used_during_setup_is_spent(pool: PgPool) {
    let enrolment_clock = Arc::new(ManualClock::new(test_start()));
    let admin = seed_admin(&pool, &enrolment_clock, "ran").await;
    let (state, clock) = common::test_state(pool);
    let app = test_app(state, client());

    let setup_code = admin.code_now(&clock);
    assert_eq!(login(&app, "ran", PASSWORD, &setup_code).await.status, StatusCode::UNAUTHORIZED);
    clock.advance(Duration::from_secs(30));
    assert_eq!(login(&app, "ran", PASSWORD, &admin.code_now(&clock)).await.status, StatusCode::OK);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_recovery_code_works_once_in_any_spelling(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let code = &fixture.admin.recovery_codes[3];
    let sloppy = code.replace('-', " ").to_lowercase().replace('1', "l").replace('0', "o");

    let reply = login(&fixture.app, "ran", PASSWORD, &sloppy).await;
    assert_eq!(reply.status, StatusCode::OK, "{sloppy}: {:?}", reply.body);
    assert_eq!(login(&fixture.app, "ran", PASSWORD, code).await.status, StatusCode::UNAUTHORIZED);

    let unused: i64 = sqlx::query_scalar("SELECT count(*) FROM admin_recovery_codes WHERE used_at IS NULL")
        .fetch_one(fixture.pool())
        .await
        .unwrap();
    assert_eq!(unused, 9);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn malformed_requests_are_invalid_and_do_not_count_as_failures(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let long_name = "a".repeat(65);
    let bodies = [
        json!("username"),
        json!({ "username": "ran", "password": PASSWORD }),
        json!({ "username": "   ", "password": PASSWORD, "code": "123456" }),
        json!({ "username": long_name, "password": PASSWORD, "code": "123456" }),
        json!({ "username": "ran", "password": PASSWORD, "code": "1".repeat(65) }),
    ];
    for body in bodies {
        let reply = Call::new(Method::POST, "/api/admin/session").json(body.clone()).send(&fixture.app).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{body}");
        assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");
    }
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &fixture.code()).await.status, StatusCode::OK);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn passwords_and_codes_never_reach_the_logs_or_the_audit_trail(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let (logs, _guard) = capture_logs();
    let typo = "my secret password typed as a username";
    login(&fixture.app, typo, PASSWORD, "123456").await;
    login(&fixture.app, "ran", "a wrong but secret password", "654321").await;
    let code = fixture.code();
    login(&fixture.app, "ran", PASSWORD, &code).await;

    let logged = logs.text();
    assert!(logged.contains("admin login failed") && logged.contains("admin signed in"), "{logged}");
    let audit: Vec<String> =
        sqlx::query_scalar("SELECT concat_ws(' ', action, metadata::text, ip_address::text) FROM audit_logs")
            .fetch_all(fixture.pool())
            .await
            .unwrap();
    let audit = audit.join("\n");
    for secret in [typo, PASSWORD, "a wrong but secret password", "654321", &code] {
        assert!(!logged.contains(secret), "{secret:?} logged");
        assert!(!audit.contains(secret), "{secret:?} audited");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_database_outage_is_a_plain_server_error(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    fixture.pool().close().await;
    let reply = login(&fixture.app, "ran", PASSWORD, &fixture.code()).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"));
    assert!(!reply.body.to_string().contains("pool"), "{}", reply.body);
    // an outage says nothing about the password: however often it happens, nobody is locked out
    for _ in 0..10 {
        let reply = login(&fixture.app, "ran", PASSWORD, &fixture.code()).await;
        assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR, "never 429");
    }
}
