// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:30.082810
//+ Revised: 	2026/11/03 08:25:52.930632
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
