// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.378030
//+ Revised: 	2026/11/12 23:57:44.261914
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

use crate::money::Currency;
use crate::products::{MAX_OFFLINE_DAYS, MIN_OFFLINE_DAYS};

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
pub const CURRENCY_VAR: &str = "CLAVREN_CURRENCY";
pub const SIGNING_KEY_VERSION_VAR: &str = "CLAVREN_SIGNING_KEY_VERSION";
/// Read by `clavren-update-check.sh`; the server only needs to know whether it is `off`.
pub const UPDATE_SOURCE_VAR: &str = "CLAVREN_UPDATE_SOURCE";
/// Where `install.sh` put the root updater, and where it reports; only tests change these.
pub const UPDATE_SCRIPT_VAR: &str = "CLAVREN_UPDATE_SCRIPT";
pub const UPDATE_STATUS_VAR: &str = "CLAVREN_UPDATE_STATUS";
/// The web application form's Cloudflare Turnstile keys: both, or neither (no human check).
pub const TURNSTILE_SITE_KEY_VAR: &str = "CLAVREN_TURNSTILE_SITE_KEY";
pub const TURNSTILE_SECRET_VAR: &str = "CLAVREN_TURNSTILE_SECRET";
/// Where Turnstile answers are verified; only tests change it.
pub const TURNSTILE_VERIFY_URL_VAR: &str = "CLAVREN_TURNSTILE_VERIFY_URL";

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
const DEFAULT_UPDATE_SCRIPT: &str = "/usr/local/lib/clavren/clavren-update.sh";
/// In a folder only root can write (`StateDirectory=` of `clavren-update.service`): the root
/// updater never writes where the clavren user could plant a link.
const DEFAULT_UPDATE_STATUS: &str = "/var/lib/clavren-update/status.json";
pub const DEFAULT_TURNSTILE_VERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
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
    /// The offline limit new products start with; each product then has its own
    /// (`products.offline_days`), which every token of its licences lasts (capped at the licence's
    /// expiry).
    pub token_lifetime: Duration,
    /// Which signing key new tokens use (their `kid` is `v<version>`). Raised to rotate keys once
    /// every application in use trusts the next one.
    pub signing_key_version: u32,
    /// The currency the dashboard preselects for prices (`CLAVREN_CURRENCY`, EUR by default).
    /// Stored prices keep their own currency, so changing it never relabels old amounts.
    pub default_currency: Currency,
}

/// Updates are found and installed by scripts outside the server (`deploy/clavren-update*.sh`); the
/// server reads what they write and writes requests for them in `dir`.
#[derive(Clone, Debug)]
pub struct UpdateSettings {
    pub dir: PathBuf,
    /// `false` when `CLAVREN_UPDATE_SOURCE=off`.
    pub checking: bool,
    /// The root updater; when it isn't installed (a source installation), there is no Update button.
    pub installer: PathBuf,
    /// How the current or last update went, written by the root updater.
    pub status: PathBuf,
}

/// The licence application form.
#[derive(Clone, Debug, Default)]
pub struct ApplicationSettings {
    /// The web form's human check; `None` shows the form without one.
    pub turnstile: Option<TurnstileSettings>,
}

/// Cloudflare Turnstile: the site key the web form shows, and the secret the server verifies its
/// answers with.
#[derive(Clone)]
pub struct TurnstileSettings {
    pub site_key: String,
    pub secret: String,
    pub verify_url: String,
}

/// The secret is never printed.
impl fmt::Debug for TurnstileSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TurnstileSettings")
            .field("site_key", &self.site_key)
            .field("secret", &"<redacted>")
            .field("verify_url", &self.verify_url)
            .finish()
    }
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

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            dir: PathBuf::from(DEFAULT_DATA_DIR).join(UPDATES_FOLDER),
            checking: true,
            installer: PathBuf::from(DEFAULT_UPDATE_SCRIPT),
            status: PathBuf::from(DEFAULT_UPDATE_STATUS),
        }
    }
}

impl LicensingSettings {
    /// The offline days a new product starts with, unless its request says otherwise.
    pub fn new_product_offline_days(&self) -> i32 {
        // within MIN_OFFLINE_DAYS..=MAX_OFFLINE_DAYS, checked at startup
        i32::try_from(self.token_lifetime.as_secs() / (24 * 3600)).unwrap_or(MAX_OFFLINE_DAYS)
    }
}

impl Default for LicensingSettings {
    fn default() -> Self {
        Self {
            token_lifetime: days(DEFAULT_TOKEN_DAYS),
            signing_key_version: DEFAULT_SIGNING_KEY_VERSION,
            default_currency: Currency::euro(),
        }
    }
}

/// The longest any configured length of time may be.
const TEN_YEARS_IN_DAYS: u64 = 3650;

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
            .field("updates", &self.updates)
            .field("applications", &self.applications)
            .finish()
    }
}
