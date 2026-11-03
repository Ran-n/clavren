// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 13:04:41.080840
//+ Revised: 	2026/11/03 13:14:24.613092
// -----------------------------------------------------------------------------

//! The local, disposable database cluster of `scripts/local-db.ps1`.

use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::process;

/// Where `scripts/local-db.ps1` listens; keep in step with its `$Port`.
const LOCAL_CLUSTER: &str = "127.0.0.1:55432";
const DATABASE_URL_VAR: &str = "CLAVREN_DATABASE_URL";

/// The local cluster while the launcher runs. Dropping it stops the cluster if this launcher
/// started it.
pub struct LocalDatabase {
    started_by: Option<PathBuf>,
}

impl LocalDatabase {
    /// Starts the local cluster unless it is running already, or the configuration points at
    /// another database (an explicit `CLAVREN_DATABASE_URL`), which is then left to its owner.
    pub fn ensure(root: &Path) -> Result<Self, String> {
        let url = std::env::var(DATABASE_URL_VAR).unwrap_or_default();
        if !uses_local_cluster(&url) {
            tracing::info!(target: "xtask::database", "using the database in {DATABASE_URL_VAR}");
            return Ok(Self { started_by: None });
        }
        if is_listening() {
            tracing::info!(target: "xtask::database", "local database already running on {LOCAL_CLUSTER}");
            return Ok(Self { started_by: None });
        }
        tracing::info!(target: "xtask::database", "starting the local database");
        process::run(local_db(root, &[]), "scripts/local-db.ps1")?;
        Ok(Self { started_by: Some(root.to_path_buf()) })
    }
}

impl Drop for LocalDatabase {
    fn drop(&mut self) {
        let Some(root) = &self.started_by else { return };
        tracing::info!(target: "xtask::database", "stopping the local database");
        if let Err(message) = process::run(local_db(root, &["-Stop"]), "scripts/local-db.ps1 -Stop") {
            tracing::error!(target: "xtask::database", "{message}");
        }
    }
}

/// Whether a database URL is the local cluster's.
pub fn uses_local_cluster(url: &str) -> bool {
    url.split_once('@').is_some_and(|(_, rest)| rest.starts_with(&format!("{LOCAL_CLUSTER}/")))
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
    }

    #[test]
    fn any_other_database_is_not() {
        assert!(!uses_local_cluster("postgres://clavren:secret@db.example.com:5432/clavren"));
        assert!(!uses_local_cluster("postgres://clavren_local@127.0.0.1:5543/clavren_dev"));
        assert!(!uses_local_cluster("postgres://clavren_local@127.0.0.1:554320/clavren_dev"));
        assert!(!uses_local_cluster(""));
    }
}
