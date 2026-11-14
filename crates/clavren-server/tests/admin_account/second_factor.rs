// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:23.705322
//+ Revised: 	2026/11/14 12:05:23.705322
// -----------------------------------------------------------------------------

//! New recovery codes and a new authenticator app.

use super::*;

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

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn wrong_second_factors_for_new_recovery_codes_count_towards_the_limit(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    let wrong = json!({ "password": PASSWORD, "code": "000000" });
    for attempt in 0..5 {
        let reply = post(&fixture, &browser, "/api/admin/account/recovery-codes", wrong.clone()).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::UNAUTHORIZED, "invalid_credentials"), "{attempt}");
    }
    let right = json!({ "password": PASSWORD, "code": fixture.code() });
    let limited = post(&fixture, &browser, "/api/admin/account/recovery-codes", right).await;
    assert_eq!((limited.status, limited.error()), (StatusCode::TOO_MANY_REQUESTS, "rate_limited"));
    assert_eq!(audited(&fixture, "admin.recovery_codes_regenerated").await, 0);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_secret_that_is_not_a_key_is_refused_saying_so(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let browser = fixture.sign_in().await;
    let body = json!({ "password": PASSWORD, "secret": "not base32!", "code": "123456" });
    let reply =
        Call::new(Method::PUT, "/api/admin/account/totp").signed_in(&browser).json(body).send(&fixture.app).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"));
    assert!(reply.body["message"].as_str().unwrap().contains("20-byte Base32"), "{:?}", reply.body);
}
