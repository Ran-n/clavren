// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:30.139830
//+ Revised: 	2026/11/12 23:57:46.215208
// -----------------------------------------------------------------------------

//! The database enforces the rules of docs/spec.md section 7 by itself: derived licence state, one
//! active computer per licence (any number for a test licence, which stays one), an immutable
//! product code once licences exist, customers erased without breaking licences. The column lists are pinned against docs/design-decisions.md.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

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

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_product_code_is_fixed_once_it_has_licences(pool: PgPool) {
    let product_id = product(&pool, "avalio-lite").await;
    let rename = "UPDATE products SET code = $2 WHERE id = $1";
    sqlx::query(rename).bind(product_id).bind("avalio").execute(&pool).await.unwrap();

    licence(&pool, product_id, None).await;
    let error = sqlx::query(rename).bind(product_id).bind("avalio-lite").execute(&pool).await.unwrap_err();
    assert!(is_check_or_unique_violation(&error));
    sqlx::query("UPDATE products SET name = 'Renamed' WHERE id = $1").bind(product_id).execute(&pool).await.unwrap();
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_product_with_licences_cannot_be_deleted(pool: PgPool) {
    let product_id = product(&pool, "avalio-lite").await;
    licence(&pool, product_id, None).await;
    let error = sqlx::query("DELETE FROM products WHERE id = $1").bind(product_id).execute(&pool).await.unwrap_err();
    assert_eq!(error.as_database_error().and_then(|error| error.code()).as_deref(), Some("23503"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn invalid_product_codes_are_rejected(pool: PgPool) {
    for code in ["", "Avalio", "has space", "-leading-dash", "ñ"] {
        let result = sqlx::query("INSERT INTO products (code, name) VALUES ($1, 'x')").bind(code).execute(&pool).await;
        assert!(is_check_or_unique_violation(&result.unwrap_err()), "{code:?} accepted");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn key_instructions_are_optional_and_at_most_2000_characters(pool: PgPool) {
    let insert = "INSERT INTO products (code, name, key_instructions) VALUES ($1, 'x', $2)";
    for (code, text) in [("none", None), ("full", Some("é".repeat(2000)))] {
        sqlx::query(insert).bind(code).bind(text).execute(&pool).await.unwrap();
    }
    let result = sqlx::query(insert).bind("long").bind("x".repeat(2001)).execute(&pool).await;
    assert!(is_check_or_unique_violation(&result.unwrap_err()));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn offline_days_default_to_30_and_stay_within_2_to_3650(pool: PgPool) {
    let id = product(&pool, "avalio-lite").await;
    let read = "SELECT offline_days FROM products WHERE id = $1";
    let days: i32 = sqlx::query_scalar(read).bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(days, 30);
    let set = "UPDATE products SET offline_days = $2 WHERE id = $1";
    for bad in [Some(1), Some(3651), None] {
        let error = sqlx::query(set).bind(id).bind(bad).execute(&pool).await.unwrap_err();
        let code = error.as_database_error().and_then(|error| error.code()).map(|code| code.to_string());
        assert!(matches!(code.as_deref(), Some("23514" | "23502")), "{bad:?}: {code:?}");
    }
    for good in [2, 3650] {
        sqlx::query(set).bind(id).bind(good).execute(&pool).await.unwrap();
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn prices_are_whole_minor_units_with_a_currency_and_never_on_a_test_licence(pool: PgPool) {
    let id = product(&pool, "avalio-lite").await;
    let set = "UPDATE products SET price_amount = $2, price_currency = $3 WHERE id = $1";
    for (amount, currency) in [
        (Some(1999_i64), None),
        (None, Some("EUR")),
        (Some(-1), Some("EUR")),
        (Some(100_000_000), Some("EUR")),
        (Some(1), Some("eur")),
    ] {
        let error = sqlx::query(set).bind(id).bind(amount).bind(currency).execute(&pool).await.unwrap_err();
        assert!(is_check_or_unique_violation(&error), "{amount:?} {currency:?}");
    }
    sqlx::query(set).bind(id).bind(1999_i64).bind("EUR").execute(&pool).await.unwrap();
    let test = test_licence(&pool, id).await;
    let priced = "UPDATE licenses SET price_amount = 1999, price_currency = 'EUR' WHERE id = $1";
    let error = sqlx::query(priced).bind(test).execute(&pool).await.unwrap_err();
    assert!(is_check_or_unique_violation(&error), "a priced test licence");
    let sold = licence(&pool, id, None).await;
    sqlx::query(priced).bind(sold).execute(&pool).await.unwrap();
    let half = "UPDATE licenses SET price_currency = NULL WHERE id = $1";
    assert!(is_check_or_unique_violation(&sqlx::query(half).bind(sold).execute(&pool).await.unwrap_err()));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn customer_emails_are_unique_ignoring_case_but_optional(pool: PgPool) {
    let insert = "INSERT INTO customers (name, email) VALUES ('Ana', $1)";
    sqlx::query(insert).bind("ana@example.com").execute(&pool).await.unwrap();
    let duplicate = sqlx::query(insert).bind("ANA@Example.com").execute(&pool).await.unwrap_err();
    assert!(is_check_or_unique_violation(&duplicate));
    sqlx::query(insert).bind(None::<String>).execute(&pool).await.unwrap();
    sqlx::query(insert).bind(None::<String>).execute(&pool).await.unwrap();
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn deleting_a_customer_keeps_their_licences(pool: PgPool) {
    let customer_id: Uuid =
        sqlx::query_scalar("INSERT INTO customers (name, email) VALUES ('Ana', 'ana@example.com') RETURNING id")
            .fetch_one(&pool)
            .await
            .unwrap();
    let licence_id = licence(&pool, product(&pool, "avalio-lite").await, Some(customer_id)).await;
    activate(&pool, licence_id).await.unwrap();

    sqlx::query("DELETE FROM customers WHERE id = $1").bind(customer_id).execute(&pool).await.unwrap();

    let owner: Option<Uuid> = sqlx::query_scalar("SELECT customer_id FROM licenses WHERE id = $1")
        .bind(licence_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owner, None);
    assert_eq!(state(&pool, licence_id).await, "redeemed");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn check_counts_are_whole_hours_with_at_least_one_check(pool: PgPool) {
    let insert = "INSERT INTO check_counts (hour, checks) VALUES ($1::timestamptz, $2)";
    sqlx::query(insert).bind("2027-03-01T09:00:00Z").bind(3i64).execute(&pool).await.unwrap();
    for (hour, checks) in [("2027-03-01T09:00:00Z", 1i64), ("2027-03-01T10:30:00Z", 1), ("2027-03-01T11:00:00Z", 0)] {
        let refused = sqlx::query(insert).bind(hour).bind(checks).execute(&pool).await.unwrap_err();
        assert!(is_check_or_unique_violation(&refused), "{hour} {checks}");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn lookup_hashes_are_unique_and_32_bytes(pool: PgPool) {
    let product_id = product(&pool, "avalio-lite").await;
    let insert = "INSERT INTO licenses (product_id, key_lookup) VALUES ($1, $2)";
    sqlx::query(insert).bind(product_id).bind([1u8; 32].as_slice()).execute(&pool).await.unwrap();
    let duplicate = sqlx::query(insert).bind(product_id).bind([1u8; 32].as_slice()).execute(&pool).await.unwrap_err();
    assert!(is_check_or_unique_violation(&duplicate));
    let short = sqlx::query(insert).bind(product_id).bind([2u8; 31].as_slice()).execute(&pool).await.unwrap_err();
    assert!(is_check_or_unique_violation(&short));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_master_key_guard_sees_licences_and_admin_accounts(pool: PgPool) {
    assert!(!database::holds_master_key_data(&pool).await.unwrap());

    licence(&pool, product(&pool, "avalio-lite").await, None).await;
    assert!(database::holds_master_key_data(&pool).await.unwrap());

    sqlx::query("DELETE FROM licenses").execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO admin_users (username, password_hash, totp_secret_ciphertext, totp_secret_nonce)
         VALUES ('ran', 'x', '\\x00', $1)",
    )
    .bind([0u8; 24].as_slice())
    .execute(&pool)
    .await
    .unwrap();
    assert!(database::holds_master_key_data(&pool).await.unwrap());
}

/// Pins every table's columns. Adding one means checking it against the BCNF table in
/// docs/design-decisions.md (no copies of facts stored elsewhere) and updating both.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn table_columns_match_the_documented_schema(pool: PgPool) {
    let expected: &[(&str, &[&str])] = &[
        (
            "activations",
            &[
                "id",
                "license_id",
                "device_public_key",
                "device_fingerprint_hash",
                "device_name",
                "platform",
                "app_version",
                "activated_at",
                "last_checked_at",
                "revoked_at",
                "revocation_reason",
            ],
        ),
        ("admin_preferences", &["admin_user_id", "preferences", "updated_at"]),
        ("admin_recovery_codes", &["id", "admin_user_id", "code_hash", "used_at"]),
        (
            "admin_sessions",
            &["id", "admin_user_id", "token_hash", "created_at", "last_seen_at", "expires_at", "revoked_at"],
        ),
        (
            "admin_users",
            &[
                "id",
                "username",
                "password_hash",
                "totp_secret_ciphertext",
                "totp_secret_nonce",
                "totp_last_used_step",
                "created_at",
                "last_login_at",
                "disabled_at",
            ],
        ),
        (
            "audit_logs",
            &["id", "admin_user_id", "action", "entity_type", "entity_id", "metadata", "ip_address", "created_at"],
        ),
        ("backup_runs", &["id", "finished_at", "archive_name", "size_bytes"]),
        ("check_counts", &["hour", "checks"]),
        ("customers", &["id", "name", "email", "notes", "created_at", "updated_at"]),
        (
            "license_applications",
            &[
                "id",
                "product_id",
                "name",
                "email",
                "message",
                "source",
                "secret_hash",
                "created_at",
                "decided_at",
                "decision",
                "license_id",
                "rejection_reason",
                "key_collected_at",
            ],
        ),
        ("license_key_secrets", &["license_id", "ciphertext", "nonce", "encryption_version"]),
        (
            "licenses",
            &[
                "id",
                "product_id",
                "customer_id",
                "key_lookup",
                "created_at",
                "expires_at",
                "revoked_at",
                "revocation_reason",
                "internal_notes",
                "is_test",
                "price_amount",
                "price_currency",
            ],
        ),
        (
            "products",
            &[
                "id",
                "code",
                "name",
                "description",
                "message_template",
                "is_active",
                "created_at",
                "updated_at",
                "accepts_applications",
                "key_instructions",
                "offline_days",
                "price_amount",
                "price_currency",
            ],
        ),
        ("verification_challenges", &["id", "activation_id", "nonce_hash", "created_at", "expires_at", "consumed_at"]),
    ];
    for (table, columns) in expected {
        let actual: Vec<String> = sqlx::query_scalar(
            "SELECT column_name::text FROM information_schema.columns
             WHERE table_schema = 'public' AND table_name = $1 ORDER BY ordinal_position",
        )
        .bind(table)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(actual, *columns, "columns of {table}");
    }
}

/// An application for `product_id` from `email`, decided as `decision` says (`None`: waiting).
async fn application(
    pool: &PgPool,
    product_id: Uuid,
    email: &str,
    decision: Option<&str>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO license_applications (product_id, name, email, source, secret_hash, decided_at, decision)
         VALUES ($1, 'Ana', $2, 'app', $3, CASE WHEN $4::text IS NULL THEN NULL ELSE now() END, $4)
         RETURNING id",
    )
    .bind(product_id)
    .bind(email)
    .bind(Uuid::now_v7().as_bytes().repeat(2))
    .bind(decision)
    .fetch_one(pool)
    .await
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn one_application_waits_per_product_and_address(pool: PgPool) {
    let (avalio, other) = (product(&pool, "avalio-lite").await, product(&pool, "other").await);
    application(&pool, avalio, "ana@example.com", None).await.unwrap();
    let again = application(&pool, avalio, "ANA@example.com", None).await.unwrap_err();
    assert!(is_check_or_unique_violation(&again));
    application(&pool, other, "ana@example.com", None).await.unwrap();
    application(&pool, avalio, "ana@example.com", Some("rejected")).await.unwrap();
    application(&pool, avalio, "ana@example.com", Some("approved")).await.unwrap();
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_applications_decision_and_its_details_go_together(pool: PgPool) {
    let product_id = product(&pool, "avalio-lite").await;
    let waiting = application(&pool, product_id, "ana@example.com", None).await.unwrap();
    let rejected = application(&pool, product_id, "bo@example.com", Some("rejected")).await.unwrap();
    let licence_id = licence(&pool, product_id, None).await;
    for (id, change) in [
        (waiting, "decision = 'approved'"),
        (waiting, "decided_at = now()"),
        (waiting, "rejection_reason = 'no'"),
        (waiting, "key_collected_at = now()"),
        (rejected, "license_id = $2"),
        (rejected, "key_collected_at = now()"),
        (rejected, "decision = 'maybe'"),
        (waiting, "source = 'fax'"),
    ] {
        let error = sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE license_applications SET {change} WHERE id = $1")))
            .bind(id)
            .bind(licence_id)
            .execute(&pool)
            .await
            .unwrap_err();
        assert!(is_check_or_unique_violation(&error), "{change}");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_approval_outlives_its_licence_and_goes_with_its_product(pool: PgPool) {
    let product_id = product(&pool, "avalio-lite").await;
    let licence_id = licence(&pool, product_id, None).await;
    let approved = application(&pool, product_id, "ana@example.com", Some("approved")).await.unwrap();
    sqlx::query("UPDATE license_applications SET license_id = $2 WHERE id = $1")
        .bind(approved)
        .bind(licence_id)
        .execute(&pool)
        .await
        .unwrap();
    let second = application(&pool, product_id, "bo@example.com", Some("approved")).await.unwrap();
    let taken = sqlx::query("UPDATE license_applications SET license_id = $2 WHERE id = $1")
        .bind(second)
        .bind(licence_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(is_check_or_unique_violation(&taken), "one application per licence");
    sqlx::query("DELETE FROM licenses WHERE id = $1").bind(licence_id).execute(&pool).await.unwrap();
    let licence: Option<Uuid> = sqlx::query_scalar("SELECT license_id FROM license_applications WHERE id = $1")
        .bind(approved)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(licence, None, "the approval stays without its licence");
    sqlx::query("DELETE FROM products WHERE id = $1").bind(product_id).execute(&pool).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM license_applications").fetch_one(&pool).await.unwrap();
    assert_eq!(left, 0);
}
