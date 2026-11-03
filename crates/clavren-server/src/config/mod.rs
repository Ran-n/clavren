// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.378030
//+ Revised: 	2026/11/03 04:54:19.212422
// -----------------------------------------------------------------------------

//! Server configuration, read from `CLAVREN_*` environment variables (on the Pi, from
//! `/etc/clavren/clavren.env`). Every value is validated here, so a bad setting stops the server
//! at startup with a message naming the variable, never later.

mod parse;
#[cfg(test)]
mod tests;

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use axum::http::HeaderName;
use sqlx::postgres::PgConnectOptions;

pub const DATABASE_URL_VAR: &str = "CLAVREN_DATABASE_URL";
pub const LISTEN_VAR: &str = "CLAVREN_LISTEN";
pub const DATA_DIR_VAR: &str = "CLAVREN_DATA_DIR";
pub const PUBLIC_URL_VAR: &str = "CLAVREN_PUBLIC_URL";
pub const TRUSTED_PROXIES_VAR: &str = "CLAVREN_TRUSTED_PROXIES";
pub const CLIENT_IP_HEADER_VAR: &str = "CLAVREN_CLIENT_IP_HEADER";
pub const SESSION_IDLE_HOURS_VAR: &str = "CLAVREN_SESSION_IDLE_HOURS";
pub const SESSION_MAX_DAYS_VAR: &str = "CLAVREN_SESSION_MAX_DAYS";
pub const LOGIN_MAX_FAILURES_VAR: &str = "CLAVREN_LOGIN_MAX_FAILURES";
pub const LOGIN_WINDOW_MINUTES_VAR: &str = "CLAVREN_LOGIN_WINDOW_MINUTES";
pub const TOKEN_DAYS_VAR: &str = "CLAVREN_TOKEN_DAYS";
pub const SIGNING_KEY_VERSION_VAR: &str = "CLAVREN_SIGNING_KEY_VERSION";

/// Localhost only: on the Pi, `cloudflared` is the only thing that should reach Clavren.
const DEFAULT_LISTEN: &str = "127.0.0.1:8080";
const DEFAULT_DATA_DIR: &str = "/var/lib/clavren";
/// `cloudflared` runs on the same machine and connects from localhost.
const DEFAULT_TRUSTED_PROXIES: &str = "127.0.0.1,::1";
const DEFAULT_CLIENT_IP_HEADER: &str = "cf-connecting-ip";
const DEFAULT_SESSION_IDLE_HOURS: u64 = 12;
const DEFAULT_SESSION_MAX_DAYS: u64 = 7;
const DEFAULT_LOGIN_MAX_FAILURES: u32 = 5;
const DEFAULT_LOGIN_WINDOW_MINUTES: u64 = 15;
const DEFAULT_TOKEN_DAYS: u64 = 30;
const DEFAULT_SIGNING_KEY_VERSION: u32 = 1;

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
}

/// Which peers may tell Clavren the real client address, and in which header.
#[derive(Clone, Debug)]
pub struct ProxySettings {
    pub trusted: Vec<IpAddr>,
    pub client_ip_header: HeaderName,
}

#[derive(Clone, Debug)]
pub struct AdminAuthSettings {
    /// A session not used for this long ends.
    pub session_idle: Duration,
    /// A session ends this long after login, used or not.
    pub session_lifetime: Duration,
    pub login_max_failures: u32,
    pub login_failure_window: Duration,
    /// `Secure` cookies, sent over HTTPS only: on whenever the public URL is https://.
    pub secure_cookies: bool,
}

#[derive(Clone, Debug)]
pub struct LicensingSettings {
    /// How long an activation token works offline (capped at the licence's expiry).
    pub token_lifetime: Duration,
    /// Which signing key new tokens use (their `kid` is `v<version>`). Raised to rotate keys once
    /// every application in use trusts the next one.
    pub signing_key_version: u32,
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
        Ok(Self {
            database,
            listen_address: parse::socket_address(LISTEN_VAR, read(LISTEN_VAR).as_deref().unwrap_or(DEFAULT_LISTEN))?,
            data_dir: PathBuf::from(read(DATA_DIR_VAR).unwrap_or_else(|| DEFAULT_DATA_DIR.into())),
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
                session_idle: hours(parse::positive(
                    SESSION_IDLE_HOURS_VAR,
                    read(SESSION_IDLE_HOURS_VAR),
                    DEFAULT_SESSION_IDLE_HOURS,
                )?),
                session_lifetime: days(parse::positive(
                    SESSION_MAX_DAYS_VAR,
                    read(SESSION_MAX_DAYS_VAR),
                    DEFAULT_SESSION_MAX_DAYS,
                )?),
                login_max_failures: parse::positive(
                    LOGIN_MAX_FAILURES_VAR,
                    read(LOGIN_MAX_FAILURES_VAR),
                    DEFAULT_LOGIN_MAX_FAILURES,
                )?,
                login_failure_window: minutes(parse::positive(
                    LOGIN_WINDOW_MINUTES_VAR,
                    read(LOGIN_WINDOW_MINUTES_VAR),
                    DEFAULT_LOGIN_WINDOW_MINUTES,
                )?),
                secure_cookies: public_url.starts_with("https://"),
            },
            licensing: LicensingSettings {
                token_lifetime: days(parse::positive(TOKEN_DAYS_VAR, read(TOKEN_DAYS_VAR), DEFAULT_TOKEN_DAYS)?),
                signing_key_version: parse::positive(
                    SIGNING_KEY_VERSION_VAR,
                    read(SIGNING_KEY_VERSION_VAR),
                    DEFAULT_SIGNING_KEY_VERSION,
                )?,
            },
            public_url,
        })
    }
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            trusted: vec![IpAddr::from([127, 0, 0, 1]), IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1])],
            client_ip_header: HeaderName::from_static(DEFAULT_CLIENT_IP_HEADER),
        }
    }
}

impl Default for AdminAuthSettings {
    fn default() -> Self {
        Self {
            session_idle: hours(DEFAULT_SESSION_IDLE_HOURS),
            session_lifetime: days(DEFAULT_SESSION_MAX_DAYS),
            login_max_failures: DEFAULT_LOGIN_MAX_FAILURES,
            login_failure_window: minutes(DEFAULT_LOGIN_WINDOW_MINUTES),
            secure_cookies: true,
        }
    }
}

impl Default for LicensingSettings {
    fn default() -> Self {
        Self { token_lifetime: days(DEFAULT_TOKEN_DAYS), signing_key_version: DEFAULT_SIGNING_KEY_VERSION }
    }
}

fn minutes(count: u64) -> Duration {
    Duration::from_secs(count * 60)
}

fn hours(count: u64) -> Duration {
    minutes(count * 60)
}

fn days(count: u64) -> Duration {
    hours(count * 24)
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
            .finish()
    }
}
