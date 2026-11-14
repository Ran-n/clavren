// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:24.876072
//+ Revised: 	2026/11/14 12:05:24.876072
// -----------------------------------------------------------------------------

//! The web form's human check (Cloudflare Turnstile).

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_web_form_passes_the_human_check(pool: PgPool) {
    let applying = Applying::with_settings(pool, fake_turnstile().await).await;
    for token in [None, Some(""), Some("bad-answer"), Some(&*"x".repeat(2049))] {
        let reply = applying.apply(json!({ "source": "web", "turnstile_token": token })).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::FORBIDDEN, "human_check_failed"), "{token:?}");
    }
    let (id, _) = applying.applied(json!({ "source": "web", "turnstile_token": GOOD_ANSWER })).await;
    assert_eq!(applying.application(&id).await["source"], "web");
    // the failed checks counted against the address's five an hour
    assert_eq!(applying.apply(json!({ "email": "app@example.com" })).await.error(), "rate_limited");
    applying.licensing.admin.fixture.clock.advance(Duration::from_secs(3600));
    // an application's own request has no human check: the rate limit and the one-waiting rule hold it
    assert_eq!(applying.apply(json!({ "email": "app@example.com" })).await.status, StatusCode::CREATED);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_human_check_that_cannot_run_is_the_servers_fault(pool: PgPool) {
    let fake = fake_turnstile().await;
    let url = fake.applications.turnstile.as_ref().unwrap().verify_url.clone();
    let applying = Applying::with_settings(pool, turnstile_settings(&url, "a-wrong-secret")).await;
    let reply = applying.apply(json!({ "source": "web", "turnstile_token": GOOD_ANSWER })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_human_check_that_cannot_reach_cloudflare_takes_nothing(pool: PgPool) {
    let applying = Applying::with_settings(pool, turnstile_settings("http://127.0.0.1:9/siteverify", "secret")).await;
    let reply = applying.apply(json!({ "source": "web", "turnstile_token": GOOD_ANSWER })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"));
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM license_applications")
        .fetch_one(applying.admin().pool())
        .await
        .unwrap();
    assert_eq!(stored, 0);
}

/// Cloudflare answering with an error page, or with something that isn't its answer, is the
/// server's problem too: nothing is taken, and the applicant isn't told they failed the check.
async fn a_broken_human_check_takes_nothing(pool: PgPool, status: StatusCode, body: &'static str) {
    let settings = common::applying::broken_turnstile(status, body).await;
    let applying = Applying::with_settings(pool, settings).await;
    let reply = applying.apply(json!({ "source": "web", "turnstile_token": GOOD_ANSWER })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"));
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM license_applications")
        .fetch_one(applying.admin().pool())
        .await
        .unwrap();
    assert_eq!(stored, 0);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_human_check_answering_with_an_error_takes_nothing(pool: PgPool) {
    a_broken_human_check_takes_nothing(pool, StatusCode::INTERNAL_SERVER_ERROR, "{\"success\":true}").await;
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_human_check_answering_with_something_else_takes_nothing(pool: PgPool) {
    a_broken_human_check_takes_nothing(pool, StatusCode::OK, "<html>busy</html>").await;
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn without_turnstile_keys_the_web_form_has_no_human_check(pool: PgPool) {
    let applying = Applying::new(pool).await;
    assert_eq!(applying.apply(json!({ "source": "web" })).await.status, StatusCode::CREATED);
}
