// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.912555
//+ Revised: 	2026/11/03 04:54:22.917215
// -----------------------------------------------------------------------------

//! Starting and stopping the server: connect and migrate, load (or create) the master key, listen,
//! serve, and shut down cleanly on SIGTERM or Ctrl+C.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::crypto::key_ring::KeyRing;
use crate::crypto::master_key::{self, MasterKey, MasterKeyError, MasterKeyFile};
use crate::database::{self, DatabaseError};
use crate::http::{self, AppState, Settings};
use crate::maintenance;

#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    MasterKey(#[from] MasterKeyError),
    #[error("could not listen on {address}: {source}")]
    Listen { address: SocketAddr, source: io::Error },
    #[error("the server stopped with an error: {0}")]
    Serve(#[source] io::Error),
    #[error("reading the master key was interrupted: {0}")]
    Interrupted(String),
}

/// A server that is connected, migrated, holds its keys and listens, but doesn't serve yet.
pub struct Server {
    listener: TcpListener,
    state: AppState,
}

impl Server {
    pub async fn prepare(config: &Config) -> Result<Self, StartupError> {
        Self::prepare_with_clock(config, Arc::new(SystemClock)).await
    }

    /// [`Server::prepare`] with another clock: the dashboard's end-to-end tests run the server
    /// at a fixed time, so their screenshots never change.
    pub async fn prepare_with_clock(config: &Config, clock: Arc<dyn Clock>) -> Result<Self, StartupError> {
        tracing::info!(data_dir = %config.data_dir.display(), "starting");
        let (pool, keys) = open(config).await?;
        let listener = TcpListener::bind(config.listen_address)
            .await
            .map_err(|source| StartupError::Listen { address: config.listen_address, source })?;
        Ok(Self { listener, state: AppState::new(pool, keys, clock, Settings::from(config)) })
    }

    pub fn local_address(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Serves until `shutdown` completes, lets in-flight requests finish (each is capped by the
    /// request timeout), stops the maintenance task, then closes the database pool.
    pub async fn serve(self, shutdown: impl Future<Output = ()> + Send + 'static) -> Result<(), StartupError> {
        let Self { listener, state } = self;
        let pool = state.pool.clone();
        let stop_maintenance = CancellationToken::new();
        let maintenance = maintenance::spawn(state.clone(), stop_maintenance.clone(), maintenance::INTERVAL);
        if let Ok(address) = listener.local_addr() {
            tracing::info!(%address, "listening");
        }
        let app = http::router(state).into_make_service_with_connect_info::<SocketAddr>();
        let served = axum::serve(listener, app).with_graceful_shutdown(shutdown).await;
        tracing::info!("stopped accepting requests; stopping background tasks");
        stop_maintenance.cancel();
        if let Err(error) = maintenance.await {
            tracing::error!(%error, "the maintenance task ended abnormally");
        }
        tracing::info!("closing the database pool");
        pool.close().await;
        served.map_err(StartupError::Serve)?;
        tracing::info!("stopped");
        Ok(())
    }
}

/// Connects and migrates the database and loads the master key, in parallel: everything the
/// server and the `clavren-admin` CLI need before they can do anything.
pub async fn open(config: &Config) -> Result<(PgPool, KeyRing), StartupError> {
    let (key_file, pool) =
        tokio::join!(read_master_key(&config.data_dir), database::connect_and_migrate(config.database.clone()));
    let pool = pool?;
    tracing::info!("database connected and up to date");
    let master = resolve_master_key(key_file?, &pool, &config.data_dir).await?;
    Ok((pool, KeyRing::new(master)))
}

async fn read_master_key(data_dir: &Path) -> Result<MasterKeyFile, StartupError> {
    let data_dir = data_dir.to_path_buf();
    tokio::task::spawn_blocking(move || master_key::read(&data_dir))
        .await
        .map_err(|error| StartupError::Interrupted(error.to_string()))?
        .map_err(StartupError::from)
}

/// A missing master key is only created when nothing in the database depends on it.
async fn resolve_master_key(file: MasterKeyFile, pool: &PgPool, data_dir: &Path) -> Result<MasterKey, StartupError> {
    match file {
        MasterKeyFile::Found(key) => {
            tracing::info!("master key loaded");
            Ok(key)
        }
        MasterKeyFile::Missing if database::holds_master_key_data(pool).await? => {
            Err(MasterKeyError::MissingWithEncryptedData { path: master_key::master_key_path(data_dir) }.into())
        }
        MasterKeyFile::Missing => {
            let key = master_key::create(data_dir)?;
            tracing::warn!(
                path = %master_key::master_key_path(data_dir).display(),
                "created a new master key; back it up together with the database"
            );
            Ok(key)
        }
    }
}

/// Completes on Ctrl+C, or on SIGTERM (systemd stopping the service) on Unix.
pub async fn shutdown_signal() {
    let interrupt = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "could not listen for Ctrl+C");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "could not listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
    tracing::info!("shutdown requested");
}
