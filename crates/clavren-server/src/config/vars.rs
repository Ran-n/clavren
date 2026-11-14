// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:42:03.486060
//+ Revised: 	2026/11/14 11:42:03.486060
// -----------------------------------------------------------------------------

//! The names of the environment variables the server reads.

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
