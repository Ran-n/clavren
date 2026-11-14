// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:42:03.691440
//+ Revised: 	2026/11/14 11:42:03.691440
// -----------------------------------------------------------------------------

//! The settings each part of the server takes, with the values used when a variable is not set.

use super::UPDATES_FOLDER;
use crate::money::Currency;
use crate::products::MAX_OFFLINE_DAYS;
use axum::http::HeaderName;
use std::fmt;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Localhost only: on the Pi, `cloudflared` is the only thing that should reach Clavren.
pub(super) const DEFAULT_LISTEN: &str = "127.0.0.1:8080";

pub(super) const DEFAULT_DATA_DIR: &str = "/var/lib/clavren";

/// `cloudflared` runs on the same machine and connects from localhost.
pub(super) const DEFAULT_TRUSTED_PROXIES: &str = "127.0.0.1,::1";

pub(super) const DEFAULT_CLIENT_IP_HEADER: &str = "cf-connecting-ip";

pub(super) const DEFAULT_SESSION_IDLE_HOURS: u64 = 12;

pub(super) const DEFAULT_SESSION_MAX_DAYS: u64 = 7;

pub(super) const DEFAULT_LOGIN_MAX_FAILURES: u32 = 5;

pub(super) const DEFAULT_LOGIN_WINDOW_MINUTES: u64 = 15;

pub(super) const DEFAULT_TOKEN_DAYS: u64 = 30;

pub(super) const DEFAULT_SIGNING_KEY_VERSION: u32 = 1;

pub(super) const DEFAULT_UPDATE_SCRIPT: &str = "/usr/local/lib/clavren/clavren-update.sh";

/// In a folder only root can write (`StateDirectory=` of `clavren-update.service`): the root
/// updater never writes where the clavren user could plant a link.
pub(super) const DEFAULT_UPDATE_STATUS: &str = "/var/lib/clavren-update/status.json";

pub const DEFAULT_TURNSTILE_VERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

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
pub(super) const TEN_YEARS_IN_DAYS: u64 = 3650;

pub(super) fn minutes(count: u64) -> Duration {
    Duration::from_secs(count * 60)
}

pub(super) fn hours(count: u64) -> Duration {
    minutes(count * 60)
}

pub(super) fn days(count: u64) -> Duration {
    hours(count * 24)
}
