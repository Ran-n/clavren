// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:28.395177
//+ Revised: 	2026/11/14 12:05:28.395177
// -----------------------------------------------------------------------------

//! Checking now, checking turned off, and who may ask.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn check_now_writes_a_request_for_the_check(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    let reply = Call::new(Method::POST, "/api/admin/updates/check")
        .signed_in(&updates.admin.browser)
        .csrf("wrong")
        .send(&updates.admin.fixture.app)
        .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert!(!exists(&updates.dir, "check-request"));

    let reply = updates.admin.call(Method::POST, "/api/admin/updates/check", None).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED, "{:?}", reply.body);
    assert!(exists(&updates.dir, "check-request"));
    assert!(!exists(&updates.dir, "check-request.new"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn with_checking_off_nothing_is_checked_or_offered(pool: PgPool) {
    let updates = Updates::new(pool, false, true).await;
    updates.latest(&newer(), json!({}));
    let body = updates.get().await;
    assert_eq!((&body["checking"], &body["available"]), (&json!("off"), &Value::Null));
    let reply = updates.admin.call(Method::POST, "/api/admin/updates/check", None).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "updates_off"));
    assert!(!exists(&updates.dir, "check-request"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn updates_need_a_session(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    for (method, path) in [
        (Method::GET, "/api/admin/updates"),
        (Method::POST, "/api/admin/updates/check"),
        (Method::POST, "/api/admin/updates/install"),
    ] {
        let reply = Call::new(method, path).send(&updates.admin.fixture.app).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{path}");
    }
}
