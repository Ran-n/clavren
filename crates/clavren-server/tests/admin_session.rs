// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.761243
//+ Revised: 	2026/11/03 04:54:23.366647
// -----------------------------------------------------------------------------

//! Admin sessions over HTTP: the cookie and CSRF checks, idle and absolute expiry, signing out, and
//! signing out everywhere else.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use std::time::Duration;

use axum::http::{Method, StatusCode, header};
use clavren::admin_cli::Command;
use common::admin::run_cli;
use common::api::{Browser, Call, Fixture};
use sqlx::PgPool;

const HOUR: Duration = Duration::from_secs(3600);

async fn whoami(fixture: &Fixture, browser: &Browser) -> StatusCode {
    Call::new(Method::GET, "/api/admin/session").signed_in(browser).send(&fixture.app).await.status
}

async fn revoke_others(fixture: &Fixture, browser: &Browser) -> common::api::Reply {
    Call::new(Method::POST, "/api/admin/session/revoke-others").signed_in(browser).send(&fixture.app).await
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn without_a_valid_cookie_the_answer_is_unauthorized(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let reply = Call::new(Method::GET, "/api/admin/session").send(&fixture.app).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::UNAUTHORIZED, "unauthorized"));
    assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");

    let forged = Browser { cookie: "forged-token".into(), csrf: "x".into() };
    assert_eq!(whoami(&fixture, &forged).await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_current_session_describes_its_admin(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    let reply = Call::new(Method::GET, "/api/admin/session").signed_in(&browser).send(&fixture.app).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["admin"]["username"], "ran");
    assert_eq!(reply.body["csrf_token"], browser.csrf.as_str(), "a reload gets the same CSRF token back");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_unused_session_ends_after_the_idle_time(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    fixture.clock.advance(12 * HOUR);
    assert_eq!(whoami(&fixture, &browser).await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn use_keeps_a_session_alive_until_its_lifetime_ends(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    // 15 visits 11 hours apart: never idle for 12 hours, but the seven days run out on the 16th
    for visit in 1..=15 {
        fixture.clock.advance(11 * HOUR);
        assert_eq!(whoami(&fixture, &browser).await, StatusCode::OK, "visit {visit}");
    }
    fixture.clock.advance(11 * HOUR);
    assert_eq!(whoami(&fixture, &browser).await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn signing_out_ends_the_session_and_clears_the_cookie(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    let reply = Call::new(Method::DELETE, "/api/admin/session").signed_in(&browser).send(&fixture.app).await;

    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert!(reply.headers[header::SET_COOKIE].to_str().unwrap().starts_with("clavren_session=; Path=/; Max-Age=0"));
    assert_eq!(whoami(&fixture, &browser).await, StatusCode::UNAUTHORIZED);
    let logouts: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = 'admin.logout'")
        .fetch_one(fixture.pool())
        .await
        .unwrap();
    assert_eq!(logouts, 1);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn signing_out_everywhere_else_keeps_this_session(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let laptop = fixture.sign_in().await;
    let phone = fixture.sign_in().await;
    let tablet = fixture.sign_in().await;

    let reply = revoke_others(&fixture, &laptop).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["revoked"], 2);
    assert_eq!(whoami(&fixture, &laptop).await, StatusCode::OK);
    assert_eq!(whoami(&fixture, &phone).await, StatusCode::UNAUTHORIZED);
    assert_eq!(whoami(&fixture, &tablet).await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn changes_need_this_sessions_csrf_token(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let laptop = fixture.sign_in().await;
    let phone = fixture.sign_in().await;

    for token in ["", "not-a-token", phone.csrf.as_str()] {
        let reply = Call::new(Method::POST, "/api/admin/session/revoke-others")
            .signed_in(&laptop)
            .csrf(token)
            .send(&fixture.app)
            .await;
        assert_eq!((reply.status, reply.error()), (StatusCode::FORBIDDEN, "csrf_failed"), "{token:?}");
    }
    let reply = Call::new(Method::DELETE, "/api/admin/session").signed_in(&laptop).csrf("").send(&fixture.app).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert_eq!(whoami(&fixture, &phone).await, StatusCode::OK, "nothing was revoked");
    assert_eq!(whoami(&fixture, &laptop).await, StatusCode::OK, "reading needs no token");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_disabled_admins_sessions_stop_working_at_once(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    run_cli(fixture.pool(), &fixture.clock, Command::DisableAdmin { username: "ran".into() }, "ran", vec![])
        .await
        .0
        .unwrap();
    assert_eq!(whoami(&fixture, &browser).await, StatusCode::UNAUTHORIZED);

    // enabling again doesn't bring the revoked session back
    run_cli(fixture.pool(), &fixture.clock, Command::EnableAdmin { username: "ran".into() }, "ran", vec![])
        .await
        .0
        .unwrap();
    assert_eq!(whoami(&fixture, &browser).await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn last_seen_is_written_at_most_once_a_minute(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    let last_seen = || async {
        sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>("SELECT last_seen_at FROM admin_sessions")
            .fetch_one(fixture.pool())
            .await
            .unwrap()
    };
    let signed_in_at = last_seen().await;

    fixture.clock.advance(Duration::from_secs(20)); // 50 seconds after signing in
    whoami(&fixture, &browser).await;
    assert_eq!(last_seen().await, signed_in_at);

    fixture.clock.advance(Duration::from_secs(20));
    whoami(&fixture, &browser).await;
    assert_eq!(last_seen().await, signed_in_at + Duration::from_secs(70));
}
