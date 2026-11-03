// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.736112
//+ Revised: 	2026/11/03 04:54:22.280883
// -----------------------------------------------------------------------------

//! The PostgreSQL connection pool and the schema migrations (embedded in the binary from
//! `migrations/`, applied at every start).

use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// Enough for a few thousand licences on a Raspberry Pi; PostgreSQL itself allows 100.
const MAX_CONNECTIONS: u32 = 10;
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("could not connect to the database (check CLAVREN_DATABASE_URL and that PostgreSQL is running): {0}")]
    Connect(#[source] sqlx::Error),
    #[error("could not update the database schema: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("database query failed: {0}")]
    Query(#[from] sqlx::Error),
}

pub async fn connect(options: PgConnectOptions) -> Result<PgPool, DatabaseError> {
    PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect_with(options)
        .await
        .map_err(DatabaseError::Connect)
}

pub async fn migrate(pool: &PgPool) -> Result<(), DatabaseError> {
    MIGRATOR.run(pool).await?;
    Ok(())
}

pub async fn connect_and_migrate(options: PgConnectOptions) -> Result<PgPool, DatabaseError> {
    let pool = connect(options).await?;
    migrate(&pool).await?;
    Ok(pool)
}

/// Whether anything in the database was encrypted with the master key: stored licence keys,
/// lookup hashes and admin TOTP secrets all depend on it.
pub async fn holds_master_key_data(pool: &PgPool) -> Result<bool, DatabaseError> {
    let holds: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM licenses) OR EXISTS (SELECT 1 FROM admin_users)")
            .fetch_one(pool)
            .await?;
    Ok(holds)
}

/// Whether `error` is a unique violation of `constraint`: the database refused a duplicate that
/// another request inserted between our check and our write.
pub fn violates_unique(error: &sqlx::Error, constraint: &str) -> bool {
    error.as_database_error().is_some_and(|error| error.is_unique_violation() && error.constraint() == Some(constraint))
}
