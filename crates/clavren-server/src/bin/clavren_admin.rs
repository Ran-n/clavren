// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.787308
//+ Revised: 	2026/11/03 04:54:19.137987
// -----------------------------------------------------------------------------

//! `clavren-admin`: manage administrator accounts. Uses the same `CLAVREN_*` configuration as the
//! server; on the Pi, run it as the `clavren` user so it can read the master key.

use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use clavren::admin_cli::terminal::StdTerminal;
use clavren::admin_cli::{self, Cli, CliContext};
use clavren::clock::SystemClock;
use clavren::config::Config;
use clavren::logging;
use clavren::startup;

/// The terminal is for the conversation; only problems are logged unless `CLAVREN_LOG` says more.
const CLI_LOG_FILTER: &str = "warn";

fn main() -> ExitCode {
    let cli = Cli::parse();
    let filter = std::env::var(logging::LOG_VAR).unwrap_or_else(|_| CLI_LOG_FILTER.into());
    if let Err(error) = logging::init(&filter) {
        eprintln!("clavren-admin: {error}");
        return ExitCode::FAILURE;
    }
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("clavren-admin: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<(), String> {
    let config = Config::from_env().map_err(|error| error.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("could not start the runtime: {error}"))?;
    runtime.block_on(async {
        let (pool, keys) = startup::open(&config).await.map_err(|error| error.to_string())?;
        let signing_key_version = config.licensing.signing_key_version;
        let context = CliContext { pool, keys, clock: Arc::new(SystemClock), signing_key_version };
        let result = admin_cli::run(&cli.command, &context, &mut StdTerminal).await;
        context.pool.close().await;
        result.map_err(|error| error.to_string())
    })
}
