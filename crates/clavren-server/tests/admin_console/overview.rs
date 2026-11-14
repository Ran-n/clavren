// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:24.089141
//+ Revised: 	2026/11/14 12:05:24.089141
// -----------------------------------------------------------------------------

//! The Overview: counts, backup warnings and the sums charged.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_overview_counts_and_warns_about_backups(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, activation) = activated(&licensing, 1).await;
    for _ in 0..2 {
        assert_eq!(Device::new(1).check(licensing.app(), &activation).await.status, StatusCode::OK);
    }
    licensing.issue(json!({})).await;
    let (revoked, _) = licensing.issue(json!({})).await;
    licensing.admin_post(&format!("/api/admin/licences/{revoked}/revoke"), json!({})).await;

    let overview = licensing.admin.get("/api/admin/overview").await.body;
    for (field, count) in [
        ("active_products", 1),
        ("inactive_products", 0),
        ("unused", 1),
        ("redeemed", 1),
        ("revoked", 1),
        ("active_computers", 1),
        ("computers_checked_last_24h", 1),
        ("checks_last_24h", 2),
    ] {
        assert_eq!(overview[field], count, "{field}");
    }
    assert_eq!((overview["last_backup"].clone(), overview["backup_overdue"].clone()), (Value::Null, json!(true)));
    let deleted =
        licensing.admin.call(axum::http::Method::DELETE, &format!("/api/admin/licences/{revoked}"), None).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let overview = licensing.admin.get("/api/admin/overview").await.body;
    assert_eq!(
        (overview["revoked"].clone(), overview["unused"].clone()),
        (json!(0), json!(1)),
        "a deleted licence counts nowhere"
    );

    sqlx::query(
        "INSERT INTO backup_runs (finished_at, archive_name, size_bytes) VALUES ($1, 'clavren-20270115.tar', 2048)",
    )
    .bind(licensing.now() - chrono::Duration::hours(3))
    .execute(licensing.admin.pool())
    .await
    .unwrap();
    let overview = licensing.admin.get("/api/admin/overview").await.body;
    assert_eq!(
        (overview["last_backup"]["size_bytes"].clone(), overview["backup_overdue"].clone()),
        (json!(2048), json!(false))
    );
    let backups = licensing.admin.get("/api/admin/backups").await.body;
    assert_eq!(backups["data"][0]["archive_name"], "clavren-20270115.tar");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_overview_warns_when_the_last_backup_is_two_days_old(pool: PgPool) {
    let mut licensing = Licensing::new(pool).await;
    sqlx::query("INSERT INTO backup_runs (finished_at, archive_name, size_bytes) VALUES ($1, 'old.tar', 1)")
        .bind(licensing.now())
        .execute(licensing.admin.pool())
        .await
        .unwrap();
    licensing.admin.wait(Duration::from_secs(49 * 3600)).await;
    assert_eq!(licensing.admin.get("/api/admin/overview").await.body["backup_overdue"], true);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn charged_sums_per_currency_without_test_licences(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let eur = |amount: i64| json!({ "price": { "amount": amount, "currency": "EUR" } });
    let (old, _) = licensing.issue(eur(1000)).await;
    licensing.issue(eur(1999)).await;
    licensing.issue(eur(0)).await; // given for free: counted as 0
    licensing.issue(json!({ "price": { "amount": 500, "currency": "USD" } })).await;
    licensing.issue(json!({ "price": null })).await; // not recorded
    licensing.issue(json!({ "test": true })).await; // never a sale
    sqlx::query("UPDATE licenses SET created_at = created_at - interval '31 days' WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&old).unwrap())
        .execute(licensing.admin.pool())
        .await
        .unwrap();

    let overview = licensing.admin.get("/api/admin/overview").await.body;
    assert_eq!(
        overview["charged"],
        json!([
            { "currency": "EUR", "last_30_days": 1999, "total": 2999 },
            { "currency": "USD", "last_30_days": 500, "total": 500 },
        ])
    );
    assert_eq!((&overview["unpriced"], &overview["default_currency"]), (&json!(1), &json!("EUR")));
}
