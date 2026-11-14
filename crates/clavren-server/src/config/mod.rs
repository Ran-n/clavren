// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.378030
//+ Revised: 	2026/11/14 11:42:03.776330
// -----------------------------------------------------------------------------

//! Server configuration, read from `CLAVREN_*` environment variables (on the Pi, from
//! `/etc/clavren/clavren.env`). Every value is validated here, so a bad setting stops the server
//! at startup with a message naming the variable, never later.

mod parse;
mod settings;
#[cfg(test)]
mod tests;
mod vars;

use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;

use sqlx::postgres::PgConnectOptions;

use crate::products::{MAX_OFFLINE_DAYS, MIN_OFFLINE_DAYS};
use settings::*;

pub use settings::{
    AdminAuthSettings, ApplicationSettings, DEFAULT_TURNSTILE_VERIFY_URL, LicensingSettings, ProxySettings,
    TurnstileSettings, UpdateSettings,
};
pub use vars::*;

/// The folder under the data directory the update scripts and the server share.
pub const UPDATES_FOLDER: &str = "updates";

#[derive(Clone)]
pub struct Config {
    pub database: PgConnectOptions,
    pub listen_address: SocketAddr,
    pub data_dir: PathBuf,
    /// The address applications and the dashboard use, without a trailing slash; tokens carry it
    /// as their issuer.
    pub public_url: String,
    pub proxies: ProxySettings,
    pub admin: AdminAuthSettings,
    pub licensing: LicensingSettings,
    pub updates: UpdateSettings,
    pub applications: ApplicationSettings,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("{0} is not set")]
    Missing(&'static str),
    #[error("{name} is not valid: {reason}")]
    Invalid { name: &'static str, reason: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Reads the configuration through `lookup`, so tests don't depend on the process environment.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let read = |name: &'static str| lookup(name).filter(|value| !value.trim().is_empty());
        let required = |name: &'static str| read(name).ok_or(ConfigError::Missing(name));
        let database = parse::database_url(DATABASE_URL_VAR, &required(DATABASE_URL_VAR)?)?;
        let public_url = parse::public_url(PUBLIC_URL_VAR, &required(PUBLIC_URL_VAR)?)?;
        let data_dir = PathBuf::from(read(DATA_DIR_VAR).unwrap_or_else(|| DEFAULT_DATA_DIR.into()));
        Ok(Self {
            database,
            listen_address: parse::socket_address(LISTEN_VAR, read(LISTEN_VAR).as_deref().unwrap_or(DEFAULT_LISTEN))?,
            updates: UpdateSettings {
                dir: data_dir.join(UPDATES_FOLDER),
                checking: parse::update_source(UPDATE_SOURCE_VAR, read(UPDATE_SOURCE_VAR).as_deref())?,
                installer: PathBuf::from(read(UPDATE_SCRIPT_VAR).unwrap_or_else(|| DEFAULT_UPDATE_SCRIPT.into())),
                status: PathBuf::from(read(UPDATE_STATUS_VAR).unwrap_or_else(|| DEFAULT_UPDATE_STATUS.into())),
            },
            data_dir,
            proxies: ProxySettings {
                trusted: parse::ip_list(
                    TRUSTED_PROXIES_VAR,
                    read(TRUSTED_PROXIES_VAR).as_deref().unwrap_or(DEFAULT_TRUSTED_PROXIES),
                )?,
                client_ip_header: parse::header_name(
                    CLIENT_IP_HEADER_VAR,
                    read(CLIENT_IP_HEADER_VAR).as_deref().unwrap_or(DEFAULT_CLIENT_IP_HEADER),
                )?,
            },
            admin: AdminAuthSettings {
                session_idle: hours(parse::span(
                    SESSION_IDLE_HOURS_VAR,
                    read(SESSION_IDLE_HOURS_VAR),
                    DEFAULT_SESSION_IDLE_HOURS,
                    TEN_YEARS_IN_DAYS * 24,
                )?),
                session_lifetime: days(parse::span(
                    SESSION_MAX_DAYS_VAR,
                    read(SESSION_MAX_DAYS_VAR),
                    DEFAULT_SESSION_MAX_DAYS,
                    TEN_YEARS_IN_DAYS,
                )?),
                login_max_failures: parse::positive(
                    LOGIN_MAX_FAILURES_VAR,
                    read(LOGIN_MAX_FAILURES_VAR),
                    DEFAULT_LOGIN_MAX_FAILURES,
                )?,
                login_failure_window: minutes(parse::span(
                    LOGIN_WINDOW_MINUTES_VAR,
                    read(LOGIN_WINDOW_MINUTES_VAR),
                    DEFAULT_LOGIN_WINDOW_MINUTES,
                    TEN_YEARS_IN_DAYS * 24 * 60,
                )?),
                secure_cookies: public_url.starts_with("https://"),
            },
            licensing: LicensingSettings {
                // the range a product's offline_days takes, so a new product never starts outside it
                token_lifetime: days(parse::span_from(
                    TOKEN_DAYS_VAR,
                    read(TOKEN_DAYS_VAR),
                    DEFAULT_TOKEN_DAYS,
                    MIN_OFFLINE_DAYS.unsigned_abs().into(),
                    MAX_OFFLINE_DAYS.unsigned_abs().into(),
                )?),
                signing_key_version: parse::positive(
                    SIGNING_KEY_VERSION_VAR,
                    read(SIGNING_KEY_VERSION_VAR),
                    DEFAULT_SIGNING_KEY_VERSION,
                )?,
                default_currency: parse::currency(CURRENCY_VAR, read(CURRENCY_VAR))?,
            },
            applications: ApplicationSettings {
                turnstile: parse::turnstile(
                    (TURNSTILE_SITE_KEY_VAR, read(TURNSTILE_SITE_KEY_VAR)),
                    (TURNSTILE_SECRET_VAR, read(TURNSTILE_SECRET_VAR)),
                    (TURNSTILE_VERIFY_URL_VAR, read(TURNSTILE_VERIFY_URL_VAR)),
                )?,
            },
            public_url,
        })
    }
}

/// The database URL can hold a password, so it is never printed.
impl fmt::Debug for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Config")
            .field("database", &"<redacted>")
            .field("listen_address", &self.listen_address)
            .field("data_dir", &self.data_dir)
            .field("public_url", &self.public_url)
            .field("proxies", &self.proxies)
            .field("admin", &self.admin)
            .field("licensing", &self.licensing)
            .field("updates", &self.updates)
            .field("applications", &self.applications)
            .finish()
    }
}
