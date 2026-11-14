// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:24.384312
//+ Revised: 	2026/11/14 12:05:24.384312
// -----------------------------------------------------------------------------

//! The limits on failed sign-ins, by address and by username.

use super::*;

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
