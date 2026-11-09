// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.698814
//+ Revised: 	2026/11/08 14:55:45.318034
// -----------------------------------------------------------------------------

//! `POST /api/admin/session`: signing in with password and second factor, the single answer for
//! every failure, one-time codes, rate limits and client addresses behind the tunnel.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

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
async fn too_many_failures_from_one_address_are_refused_until_the_window_passes(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    for attempt in 0..5 {
        let username = format!("guess{attempt}");
        assert_eq!(login(&fixture.app, &username, "wrong password", "123456").await.status, StatusCode::UNAUTHORIZED);
    }
    let blocked = login(&fixture.app, "ran", PASSWORD, &fixture.code()).await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(blocked.error(), "rate_limited");
    assert_eq!(blocked.headers[header::RETRY_AFTER], "900");

    fixture.clock.advance(Duration::from_secs(15 * 60));
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &fixture.code()).await.status, StatusCode::OK);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn too_many_failures_for_one_username_are_refused_from_any_address(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let tunnel = test_app(fixture.state.clone(), "127.0.0.1:40000".parse().unwrap());
    let login_from = |address: &'static str, password: &'static str, code: String| {
        let tunnel = tunnel.clone();
        async move {
            Call::new(Method::POST, "/api/admin/session")
                .json(json!({ "username": "RAN", "password": password, "code": code }))
                .forwarded_for(address)
                .send(&tunnel)
                .await
        }
    };
    for last in 1..=5 {
        let address = ["203.0.113.1", "203.0.113.2", "203.0.113.3", "203.0.113.4", "203.0.113.5"][last - 1];
        assert_eq!(login_from(address, "wrong password", "123456".into()).await.status, StatusCode::UNAUTHORIZED);
    }
    let reply = login_from("203.0.113.99", PASSWORD, fixture.code()).await;
    assert_eq!(reply.status, StatusCode::TOO_MANY_REQUESTS);

    let addresses: Vec<Option<String>> = audit_rows(fixture.pool()).await.into_iter().map(|(_, ip, _)| ip).collect();
    assert_eq!(addresses[0].as_deref(), Some("203.0.113.1"), "the tunnel's client address is recorded");
}

/// Regression: the limit was checked before the password and counted after it, so guesses sent
/// in parallel all passed the check (and each ran Argon2) before any of them was counted.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn guesses_sent_in_parallel_never_pass_the_limit(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let guesses: Vec<_> = (0..20)
        .map(|_| {
            let app = fixture.app.clone();
            tokio::spawn(async move { login(&app, "ran", "wrong password", "123456").await.status })
        })
        .collect();
    let mut answers = Vec::new();
    for guess in guesses {
        answers.push(guess.await.unwrap());
    }
    let wrong = answers.iter().filter(|&&status| status == StatusCode::UNAUTHORIZED).count();
    let refused = answers.iter().filter(|&&status| status == StatusCode::TOO_MANY_REQUESTS).count();
    assert_eq!((wrong, refused), (5, 15), "{answers:?}");
}

/// Regression: the limit was counted per spelling (`ran`, `ran `, ` ran`), while the lookup
/// trimmed it, so every spelling got five more guesses at the same account.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn every_spelling_of_a_username_shares_one_limit(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let tunnel = test_app(fixture.state.clone(), "127.0.0.1:40000".parse().unwrap());
    let login_from = |address: String, username: &'static str, password: &'static str, code: String| {
        let tunnel = tunnel.clone();
        async move {
            Call::new(Method::POST, "/api/admin/session")
                .json(json!({ "username": username, "password": password, "code": code }))
                .forwarded_for(&address)
                .send(&tunnel)
                .await
        }
    };
    for (index, username) in ["ran", "ran ", " ran", "RAN  ", "\tran"].into_iter().enumerate() {
        let reply = login_from(format!("203.0.113.{}", index + 1), username, "wrong password", "123456".into()).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    }
    let reply = login_from("203.0.113.99".into(), "ran   ", PASSWORD, fixture.code()).await;
    assert_eq!(reply.status, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_success_from_an_address_leaves_it_nothing_counted(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    for _ in 0..4 {
        login(&fixture.app, "ran", "wrong password", "123456").await;
    }
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &fixture.code()).await.status, StatusCode::OK);
    fixture.clock.advance(Duration::from_secs(30));
    assert_eq!(login(&fixture.app, "nobody", "wrong password", "123456").await.status, StatusCode::UNAUTHORIZED);
    let reply = login(&fixture.app, "nobody", "wrong password", "123456").await;
    assert_eq!(reply.status, StatusCode::TOO_MANY_REQUESTS, "four failures and this one: the sign-in wasn't counted");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_success_clears_the_username_count(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    for _ in 0..4 {
        login(&fixture.app, "ran", "wrong password", "123456").await;
    }
    assert_eq!(login(&fixture.app, "ran", PASSWORD, &fixture.code()).await.status, StatusCode::OK);
    fixture.clock.advance(Duration::from_secs(30));
    let other_address = test_app(fixture.state.clone(), "198.51.100.77:1".parse().unwrap());
    for _ in 0..4 {
        login(&other_address, "ran", "wrong password", "123456").await;
    }
    assert_eq!(login(&other_address, "ran", PASSWORD, &fixture.code()).await.status, StatusCode::OK);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_forwarded_address_from_anyone_but_the_tunnel_is_ignored(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let reply = Call::new(Method::POST, "/api/admin/session")
        .json(json!({ "username": "ran", "password": PASSWORD, "code": fixture.code() }))
        .forwarded_for("203.0.113.50")
        .send(&fixture.app)
        .await;
    assert_eq!(reply.status, StatusCode::OK);
    let (_, address, _) = audit_rows(fixture.pool()).await.remove(0);
    assert_eq!(address.as_deref(), Some("198.51.100.10"));
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
}
