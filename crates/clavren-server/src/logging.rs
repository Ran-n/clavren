// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.442090
//+ Revised: 	2026/11/03 04:54:22.410612
// -----------------------------------------------------------------------------

//! Log setup. Logs go to stdout (the systemd journal on the Pi), one target per module
//! (`clavren::startup`, `clavren::http::health`…), at the level `CLAVREN_LOG` asks for.

use tracing_subscriber::EnvFilter;

pub const LOG_VAR: &str = "CLAVREN_LOG";
/// PostgreSQL's notices (e.g. "relation already exists, skipping" at every start) are routine,
/// so they only show from warning level up.
pub const DEFAULT_FILTER: &str = "info,sqlx::postgres::notice=warn";

#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    #[error("{LOG_VAR} is not a valid log filter ({0}); use a level such as info, or e.g. info,clavren=debug")]
    InvalidFilter(String),
    #[error("logging was already set up")]
    AlreadyInitialized,
}

/// The filter to use: `CLAVREN_LOG` when set, [`DEFAULT_FILTER`] otherwise.
pub fn filter_from_env() -> String {
    std::env::var(LOG_VAR).ok().filter(|value| !value.trim().is_empty()).unwrap_or_else(|| DEFAULT_FILTER.into())
}

pub fn parse_filter(filter: &str) -> Result<EnvFilter, LoggingError> {
    EnvFilter::try_new(filter).map_err(|error| LoggingError::InvalidFilter(error.to_string()))
}

/// Installs the process-wide subscriber. Called first thing in each binary.
pub fn init(filter: &str) -> Result<(), LoggingError> {
    tracing_subscriber::fmt()
        .with_env_filter(parse_filter(filter)?)
        .with_target(true)
        .try_init()
        .map_err(|_| LoggingError::AlreadyInitialized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_per_target_filters_are_accepted() {
        for filter in ["info", "debug", "warn,clavren=debug", "clavren::http=trace"] {
            assert!(parse_filter(filter).is_ok(), "{filter} rejected");
        }
    }

    #[test]
    fn the_default_filter_is_valid() {
        assert!(parse_filter(DEFAULT_FILTER).is_ok());
    }

    #[test]
    fn an_invalid_filter_names_the_variable() {
        let error = parse_filter("info,clavren=loud").unwrap_err();
        assert!(error.to_string().contains(LOG_VAR), "{error}");
    }
}
