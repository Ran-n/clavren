// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:27.810866
//+ Revised: 	2026/11/14 12:05:27.810866
// -----------------------------------------------------------------------------

//! Customers, check counts, lookup hashes and the master-key guard.

use super::*;

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
