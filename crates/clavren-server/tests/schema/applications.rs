// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:27.712831
//+ Revised: 	2026/11/14 12:05:27.712831
// -----------------------------------------------------------------------------

//! What the database itself refuses about licence applications.

use super::*;

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
