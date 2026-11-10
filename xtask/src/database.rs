// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 13:04:41.080840
//+ Revised: 	2026/11/09 22:30:57.686437
// -----------------------------------------------------------------------------

//! The local, disposable database cluster of `scripts/local-db.ps1`.

use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use crate::process;

/// Where `scripts/local-db.ps1` listens; keep in step with its `$Port`.
const LOCAL_CLUSTER: &str = "127.0.0.1:55432";
const DATABASE_URL_VAR: &str = "CLAVREN_DATABASE_URL";

/// The repository whose local cluster this launcher started, to stop it on the way out.
static STARTED: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The local cluster while the launcher runs. Dropping it stops the cluster if this launcher
/// started it.
pub struct LocalDatabase(());

impl LocalDatabase {
    /// Starts the local cluster unless it is running already, or the configuration points at
    /// another database (an explicit `CLAVREN_DATABASE_URL`), which is then left to its owner.
    pub fn ensure(root: &Path) -> Result<Self, String> {
        let url = std::env::var(DATABASE_URL_VAR).unwrap_or_default();
        if !uses_local_cluster(&url) {
            tracing::info!(target: "xtask::database", "using the database in {DATABASE_URL_VAR}");
            return Ok(Self(()));
        }
        if is_listening() {
            tracing::info!(target: "xtask::database", "local database already running on {LOCAL_CLUSTER}");
            return Ok(Self(()));
        }
        tracing::info!(target: "xtask::database", "starting the local database");
        // from here on the guard owns the cluster: a start that fails half-way is stopped too
        *STARTED.lock().unwrap_or_else(PoisonError::into_inner) = Some(root.to_path_buf());
        let database = Self(());
        process::run(local_db(root, &[]), "scripts/local-db.ps1")?;
        Ok(database)
    }
}

impl Drop for LocalDatabase {
    fn drop(&mut self) {
        stop_if_started();
    }
}

/// Stops the local cluster if this launcher started it, once.
pub fn stop_if_started() {
    let Some(root) = STARTED.lock().unwrap_or_else(PoisonError::into_inner).take() else { return };
    tracing::info!(target: "xtask::database", "stopping the local database");
    if let Err(message) = process::run(local_db(&root, &["-Stop"]), "scripts/local-db.ps1 -Stop") {
        tracing::error!(target: "xtask::database", "{message}");
    }
}

/// Whether a database URL is the local cluster's.
pub fn uses_local_cluster(url: &str) -> bool {
    // the last `@`: a password may contain one
    url.rsplit_once('@').is_some_and(|(_, rest)| rest.starts_with(&format!("{LOCAL_CLUSTER}/")))
}

fn is_listening() -> bool {
    LOCAL_CLUSTER
        .parse::<SocketAddr>()
        .is_ok_and(|address| TcpStream::connect_timeout(&address, Duration::from_millis(500)).is_ok())
}

fn local_db(root: &Path, args: &[&str]) -> Command {
    let shell = if cfg!(windows) { "powershell" } else { "pwsh" };
    let mut command = Command::new(shell);
    command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(root.join("scripts/local-db.ps1"));
    command.args(args);
    command
}

#[cfg(test)]
mod tests {
    use super::uses_local_cluster;

    #[test]
    fn the_development_url_is_the_local_cluster() {
        assert!(uses_local_cluster("postgres://clavren_local@127.0.0.1:55432/clavren_dev"));
        assert!(uses_local_cluster("postgres://clavren_local:p@ss@127.0.0.1:55432/clavren_dev"));
    }

    #[test]
    fn any_other_database_is_not() {
        assert!(!uses_local_cluster("postgres://clavren:secret@db.example.com:5432/clavren"));
        assert!(!uses_local_cluster("postgres://clavren_local@127.0.0.1:5543/clavren_dev"));
        assert!(!uses_local_cluster("postgres://clavren_local@127.0.0.1:554320/clavren_dev"));
        assert!(!uses_local_cluster(""));
        assert!(!uses_local_cluster("postgres://u:x@127.0.0.1:55432/@db.example.com:5432/clavren"));
    }
}
