// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:30.082810
//+ Revised: 	2026/11/10 17:59:22.864041
// -----------------------------------------------------------------------------

//! The migrations apply to an empty database, undo completely, apply again, and keep existing
//! data when run on a database that is already up to date.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

use clavren::database::MIGRATOR;
use sqlx::PgPool;

const TABLES: [&str; 14] = [
    "products",
    "customers",
    "licenses",
    "license_key_secrets",
    "activations",
    "verification_challenges",
    "admin_users",
    "admin_recovery_codes",
    "admin_sessions",
    "admin_preferences",
    "audit_logs",
    "backup_runs",
    "check_counts",
    "_sqlx_migrations",
];

async fn existing_tables(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables WHERE table_schema = 'public' ORDER BY 1",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn expected_tables_plus_view() -> Vec<String> {
    let mut expected: Vec<String> = TABLES.iter().map(|name| name.to_string()).collect();
    expected.push("license_states".into());
    expected.sort();
    expected
}

#[sqlx::test(migrations = false)]
async fn migrations_apply_undo_and_apply_again(pool: PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    assert_eq!(existing_tables(&pool).await, expected_tables_plus_view().await);

    MIGRATOR.undo(&pool, 0).await.unwrap();
    assert_eq!(existing_tables(&pool).await, vec!["_sqlx_migrations".to_string()]);

    MIGRATOR.run(&pool).await.unwrap();
    assert_eq!(existing_tables(&pool).await, expected_tables_plus_view().await);
}

#[sqlx::test(migrations = false)]
async fn running_the_migrations_again_keeps_the_data(pool: PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO products (code, name) VALUES ('avalio-lite', 'Avalio Lite')")
        .execute(&pool)
        .await
        .unwrap();

    MIGRATOR.run(&pool).await.unwrap();

    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM products").fetch_all(&pool).await.unwrap();
    assert_eq!(names, ["Avalio Lite"]);
}

#[sqlx::test(migrations = false)]
async fn every_migration_has_an_undo_script(pool: PgPool) {
    assert!(MIGRATOR.iter().all(|migration| migration.migration_type.is_reversible()));
    MIGRATOR.run(&pool).await.unwrap();
}

/// Undoing test licences keeps them as ordinary licences, each on its latest computer only, so
/// the one-computer index can come back.
#[sqlx::test(migrations = false)]
async fn undoing_test_licences_keeps_them_on_their_latest_computer(pool: PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    let licence: uuid::Uuid = sqlx::query_scalar(
        "WITH p AS (INSERT INTO products (code, name) VALUES ('avalio-lite', 'Avalio Lite') RETURNING id)
         INSERT INTO licenses (product_id, key_lookup, is_test) SELECT id, $1, TRUE FROM p RETURNING id",
    )
    .bind([3u8; 32].as_slice())
    .fetch_one(&pool)
    .await
    .unwrap();
    for (day, device) in [(1, 1u8), (3, 3), (2, 2)] {
        sqlx::query(
            "INSERT INTO activations (license_id, device_public_key, device_name, platform, app_version, activated_at)
             VALUES ($1, $2, 'PC', 'windows', '1.0.0', '2026-11-01'::timestamptz + make_interval(days => $3))",
        )
        .bind(licence)
        .bind([device; 32].as_slice())
        .bind(day)
        .execute(&pool)
        .await
        .unwrap();
    }

    MIGRATOR.undo(&pool, 20261109000001).await.unwrap();
    assert!(index_exists(&pool, "activations_one_active_per_license").await);
    let active: Vec<Vec<u8>> = sqlx::query_scalar("SELECT device_public_key FROM activations WHERE revoked_at IS NULL")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(active, [vec![3u8; 32]], "the latest computer stays");
    let reset: i64 = sqlx::query_scalar("SELECT count(*) FROM activations WHERE revocation_reason = 'reset'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reset, 2);
    MIGRATOR.run(&pool).await.unwrap();
    let is_test: bool = sqlx::query_scalar("SELECT is_test FROM licenses").fetch_one(&pool).await.unwrap();
    assert!(!is_test, "the licence stays, as an ordinary one");
}

async fn index_exists(pool: &PgPool, name: &str) -> bool {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE schemaname = 'public' AND indexname = $1)")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Deleting licences removes their activations, history included; the index makes that (and the
/// foreign key's own check) a lookup instead of a scan of every activation ever made.
#[sqlx::test(migrations = false)]
async fn activations_are_indexed_by_licence_and_the_index_undoes_on_its_own(pool: PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    assert!(index_exists(&pool, "activations_by_license").await);
    sqlx::query("INSERT INTO products (code, name) VALUES ('avalio-lite', 'Avalio Lite')")
        .execute(&pool)
        .await
        .unwrap();

    MIGRATOR.undo(&pool, 20261103000001).await.unwrap();
    assert!(!index_exists(&pool, "activations_by_license").await);
    assert!(index_exists(&pool, "activations_one_active_per_license").await, "the older index stays");
    MIGRATOR.run(&pool).await.unwrap();
    assert!(index_exists(&pool, "activations_by_license").await);
    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM products").fetch_all(&pool).await.unwrap();
    assert_eq!(names, ["Avalio Lite"], "stepping back and forth keeps the data");
}
