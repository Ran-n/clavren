// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.968620
//+ Revised: 	2026/11/03 04:54:19.078248
// -----------------------------------------------------------------------------

//! The Clavren server. Configuration comes from `CLAVREN_*` environment variables (see
//! `docs/development.md`); it runs until SIGTERM or Ctrl+C.

use std::process::ExitCode;

use clavren::config::Config;
use clavren::logging;
use clavren::startup::{Server, shutdown_signal};

fn main() -> ExitCode {
    // Logging comes first, so every later failure is logged; if it can't start, stderr is all
    // there is.
    if let Err(error) = logging::init(&logging::filter_from_env()) {
        eprintln!("clavren: {error}");
        return ExitCode::FAILURE;
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            tracing::error!(target: "clavren::startup", "{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let config = Config::from_env().map_err(|error| error.to_string())?;
    let runtime = tokio::runtime::Runtime::new().map_err(|error| format!("could not start the runtime: {error}"))?;
    runtime.block_on(async {
        let server = Server::prepare(&config).await.map_err(|error| error.to_string())?;
        server.serve(shutdown_signal()).await.map_err(|error| error.to_string())
    })
}
