// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 13:04:41.397543
//+ Revised: 	2026/11/03 13:14:24.949314
// -----------------------------------------------------------------------------

//! Repository tasks, run as `cargo xtask <task>` (the aliases are in `.cargo/config.toml`).
//!
//! `dev` (also `cargo dev`): everything needed to try Clavren locally, in one command. It starts
//! the local database (`scripts/local-db.ps1`), builds the dashboard when its sources changed,
//! creates the first admin when there is none, then runs the server until Ctrl+C. A database it
//! started is stopped again on the way out; one that was already running is left alone.
//! Configuration comes from `.cargo/config.toml`, like any `cargo run`.

mod admin;
mod dashboard;
mod database;
mod process;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    tracing_subscriber::fmt().with_env_filter(EnvFilter::new("info")).with_target(false).init();
    // Ctrl+C reaches the server (the same console) and the server shuts down; the launcher waits
    // for it instead of dying first, so it can stop the database afterwards.
    if let Err(error) = ctrlc::set_handler(|| {}) {
        tracing::error!(target: "xtask", "could not catch Ctrl+C: {error}");
        return ExitCode::FAILURE;
    }
    match run_task(&std::env::args().nth(1).unwrap_or_default()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            tracing::error!(target: "xtask", "{message}");
            ExitCode::FAILURE
        }
    }
}

fn run_task(task: &str) -> Result<(), String> {
    match task {
        "dev" => dev(&repository_root()),
        _ => Err(format!("unknown task {task:?}; the tasks are: dev")),
    }
}

fn dev(root: &Path) -> Result<(), String> {
    let _database = database::LocalDatabase::ensure(root)?;
    dashboard::build_if_stale(&root.join("web/admin"))?;
    process::cargo_build(root)?;
    admin::create_first_if_none(root)?;
    tracing::info!(target: "xtask", "starting the server; stop it with Ctrl+C");
    process::cargo_run(root, "clavren", &[])
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}
