// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:23.303236
//+ Revised: 	2026/11/14 17:13:09.730496
// -----------------------------------------------------------------------------

//! Activations racing each other, malformed requests and the rate limit.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn many_computers_racing_for_one_key_leave_exactly_one_activation(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, key) = licensing.issue(json!({})).await;
    let devices: Vec<Device> = (1..=8).map(Device::new).collect();
    let attempts = devices.iter().map(|device| device.activate(licensing.app(), PRODUCT, &key));
    let replies = futures_join_all(attempts).await;

    let statuses: Vec<StatusCode> = replies.iter().map(|reply| reply.status).collect();
    assert_eq!(statuses.iter().filter(|status| **status == StatusCode::CREATED).count(), 1, "{statuses:?}");
    assert!(statuses.iter().all(|status| [StatusCode::CREATED, StatusCode::CONFLICT].contains(status)), "{statuses:?}");
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM activations").fetch_one(licensing.admin.pool()).await.unwrap();
    assert_eq!(rows, 1, "no orphan activations");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn many_computers_racing_for_one_test_key_all_get_in(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, key) = licensing.issue(json!({ "test": true })).await;
    let devices: Vec<Device> = (1..=8).map(Device::new).collect();
    let replies = futures_join_all(devices.iter().map(|device| device.activate(licensing.app(), PRODUCT, &key))).await;
    let statuses: Vec<StatusCode> = replies.iter().map(|reply| reply.status).collect();
    assert!(statuses.iter().all(|status| *status == StatusCode::CREATED), "{statuses:?}");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM activations WHERE revoked_at IS NULL")
        .fetch_one(licensing.admin.pool())
        .await
        .unwrap();
    assert_eq!(rows, 8);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn bad_requests_are_refused_with_the_server_time(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, key) = licensing.issue(json!({})).await;
    let device = Device::new(1);
    let mut bodies = Vec::new();
    for (field, value) in [
        ("device_public_key", json!("too-short")),
        ("device_fingerprint", json!("AAAA")),
        ("device_name", json!("")),
        ("app_version", json!("x".repeat(201))),
        ("extra", json!(true)),
    ] {
        let mut body = device.activation_body(PRODUCT, &key);
        body[field] = value;
        bodies.push(body);
    }
    bodies.push(json!({ "product": PRODUCT }));
    for body in bodies {
        let reply = Call::new(Method::POST, "/api/v1/activations").json(body.clone()).send(licensing.app()).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{body}");
        assert!(reply.body["server_time"].is_string(), "{body}");
    }
    let huge = json!({ "padding": "x".repeat(20_000) });
    let reply = Call::new(Method::POST, "/api/v1/activations").json(huge).send(licensing.app()).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert!(
        reply.body["message"].as_str().unwrap().contains("length limit"),
        "a body over 16 KiB is refused unread: {:?}",
        reply.body
    );
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM activations").fetch_one(licensing.admin.pool()).await.unwrap();
    assert_eq!(rows, 0);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn one_address_gets_ten_activation_attempts_an_hour(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let device = Device::new(1);
    for _ in 0..10 {
        assert_eq!(device.activate(licensing.app(), PRODUCT, "BRN-0000-0000-0000-0000").await.error(), "invalid_key");
    }
    let (_, key) = licensing.issue(json!({})).await;
    let blocked = device.activate(licensing.app(), PRODUCT, &key).await;
    assert_eq!((blocked.status, blocked.error()), (StatusCode::TOO_MANY_REQUESTS, "rate_limited"));
    assert!(blocked.headers.contains_key(header::RETRY_AFTER));
    licensing.admin.fixture.clock.advance(Duration::from_secs(3600));
    assert_eq!(device.activate(licensing.app(), PRODUCT, &key).await.status, StatusCode::CREATED);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_database_outage_is_a_plain_server_error_with_the_server_time(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, key) = licensing.issue(json!({})).await;
    licensing.admin.pool().close().await;
    let reply = Device::new(1).activate(licensing.app(), PRODUCT, &key).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"));
    assert!(reply.body["server_time"].is_string(), "{:?}", reply.body);
    assert!(!reply.body.to_string().contains("pool"), "{:?}", reply.body);
}
