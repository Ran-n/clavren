// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:30.139830
//+ Revised: 	2026/11/14 12:05:27.921641
// -----------------------------------------------------------------------------

//! The database enforces the rules of docs/spec.md section 7 by itself: derived licence state, one
//! active computer per licence (any number for a test licence, which stays one), an immutable
//! product code once licences exist, customers erased without breaking licences. The column lists are pinned against docs/design-decisions.md.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod applications;
mod columns;
mod customers_and_keys;
mod products;

use clavren::database;
use sqlx::PgPool;
use uuid::Uuid;

async fn product(pool: &PgPool, code: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO products (code, name) VALUES ($1, 'A product') RETURNING id")
        .bind(code)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn licence(pool: &PgPool, product_id: Uuid, customer_id: Option<Uuid>) -> Uuid {
    sqlx::query_scalar("INSERT INTO licenses (product_id, customer_id, key_lookup) VALUES ($1, $2, $3) RETURNING id")
        .bind(product_id)
        .bind(customer_id)
        .bind(Uuid::new_v4().as_bytes().repeat(2))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn test_licence(pool: &PgPool, product_id: Uuid) -> Uuid {
    sqlx::query_scalar("INSERT INTO licenses (product_id, key_lookup, is_test) VALUES ($1, $2, TRUE) RETURNING id")
        .bind(product_id)
        .bind(Uuid::new_v4().as_bytes().repeat(2))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn activate(pool: &PgPool, licence_id: Uuid) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO activations (license_id, device_public_key, device_name, platform, app_version)
         VALUES ($1, $2, 'Teacher PC', 'windows', '1.0.0') RETURNING id",
    )
    .bind(licence_id)
    .bind([7u8; 32].as_slice())
    .fetch_one(pool)
    .await
}

async fn revoke_activation(pool: &PgPool, activation_id: Uuid) {
    sqlx::query("UPDATE activations SET revoked_at = now(), revocation_reason = 'reset' WHERE id = $1")
        .bind(activation_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn state(pool: &PgPool, licence_id: Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM license_states WHERE license_id = $1")
        .bind(licence_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn is_check_or_unique_violation(error: &sqlx::Error) -> bool {
    let code = error.as_database_error().and_then(|error| error.code()).unwrap_or_default();
    matches!(code.as_ref(), "23505" | "23514")
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn licence_state_follows_activations_and_revocation(pool: PgPool) {
    let licence_id = licence(&pool, product(&pool, "avalio-lite").await, None).await;
    assert_eq!(state(&pool, licence_id).await, "unused");

    let activation_id = activate(&pool, licence_id).await.unwrap();
    assert_eq!(state(&pool, licence_id).await, "redeemed");

    revoke_activation(&pool, activation_id).await;
    assert_eq!(state(&pool, licence_id).await, "unused", "a reset frees the licence");

    activate(&pool, licence_id).await.unwrap();
    sqlx::query("UPDATE licenses SET revoked_at = now() WHERE id = $1").bind(licence_id).execute(&pool).await.unwrap();
    assert_eq!(state(&pool, licence_id).await, "revoked", "revocation wins over an active activation");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_redemption_time_is_the_active_activations(pool: PgPool) {
    let licence_id = licence(&pool, product(&pool, "avalio-lite").await, None).await;
    let activation_id = activate(&pool, licence_id).await.unwrap();
    let (state_activation, redeemed_at_matches): (Option<Uuid>, bool) = sqlx::query_as(
        "SELECT s.active_activation_id, s.redeemed_at = a.activated_at
         FROM license_states s JOIN activations a ON a.id = s.active_activation_id WHERE s.license_id = $1",
    )
    .bind(licence_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state_activation, Some(activation_id));
    assert!(redeemed_at_matches);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_licence_has_at_most_one_active_computer(pool: PgPool) {
    let licence_id = licence(&pool, product(&pool, "avalio-lite").await, None).await;
    let first = activate(&pool, licence_id).await.unwrap();
    assert!(is_check_or_unique_violation(&activate(&pool, licence_id).await.unwrap_err()));

    revoke_activation(&pool, first).await;
    activate(&pool, licence_id).await.unwrap();
    let history: i64 = sqlx::query_scalar("SELECT count(*) FROM activations WHERE license_id = $1")
        .bind(licence_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(history, 2, "the revoked activation stays as history");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_test_licence_is_active_on_any_number_of_computers(pool: PgPool) {
    let licence_id = test_licence(&pool, product(&pool, "avalio-lite").await).await;
    let mut activations = Vec::new();
    for _ in 0..3 {
        activations.push(activate(&pool, licence_id).await.unwrap());
    }
    let (state, latest, computers): (String, Option<Uuid>, i64) = sqlx::query_as(
        "SELECT state, active_activation_id, active_computers FROM license_states WHERE license_id = $1",
    )
    .bind(licence_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((state.as_str(), latest, computers), ("redeemed", activations.last().copied(), 3));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM license_states").fetch_one(&pool).await.unwrap();
    assert_eq!(rows, 1, "one row per licence, however many computers");

    let ordinary = licence(&pool, product(&pool, "avalio-pro").await, None).await;
    let first = activate(&pool, ordinary).await.unwrap();
    revoke_activation(&pool, first).await;
    let second = activate(&pool, ordinary).await.unwrap();
    let back = sqlx::query("UPDATE activations SET revoked_at = NULL, revocation_reason = NULL WHERE id = $1")
        .bind(first)
        .execute(&pool)
        .await;
    assert!(is_check_or_unique_violation(&back.unwrap_err()), "an ended computer can't come back beside {second}");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_licence_never_becomes_or_stops_being_a_test_licence(pool: PgPool) {
    let product_id = product(&pool, "avalio-lite").await;
    for (licence_id, to) in
        [(licence(&pool, product_id, None).await, true), (test_licence(&pool, product_id).await, false)]
    {
        let change = sqlx::query("UPDATE licenses SET is_test = $2 WHERE id = $1")
            .bind(licence_id)
            .bind(to)
            .execute(&pool)
            .await;
        assert!(is_check_or_unique_violation(&change.unwrap_err()), "to {to}");
    }
    let same = sqlx::query("UPDATE licenses SET is_test = is_test, internal_notes = 'x'").execute(&pool).await;
    assert_eq!(same.unwrap().rows_affected(), 2, "other changes go through");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn revocation_time_and_reason_go_together(pool: PgPool) {
    let licence_id = licence(&pool, product(&pool, "avalio-lite").await, None).await;
    let activation_id = activate(&pool, licence_id).await.unwrap();
    let reason_only = sqlx::query("UPDATE activations SET revocation_reason = 'reset' WHERE id = $1")
        .bind(activation_id)
        .execute(&pool)
        .await;
    assert!(is_check_or_unique_violation(&reason_only.unwrap_err()));
    let unknown_reason =
        sqlx::query("UPDATE activations SET revoked_at = now(), revocation_reason = 'bored' WHERE id = $1")
            .bind(activation_id)
            .execute(&pool)
            .await;
    assert!(is_check_or_unique_violation(&unknown_reason.unwrap_err()));
    let licence_reason_only = sqlx::query("UPDATE licenses SET revocation_reason = 'refund' WHERE id = $1")
        .bind(licence_id)
        .execute(&pool)
        .await;
    assert!(is_check_or_unique_violation(&licence_reason_only.unwrap_err()));
}
