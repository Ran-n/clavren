// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:27.514199
//+ Revised: 	2026/11/14 12:05:27.514199
// -----------------------------------------------------------------------------

//! What the database itself refuses about products.

use super::*;

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
