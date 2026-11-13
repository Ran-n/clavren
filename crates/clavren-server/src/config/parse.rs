// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.848432
//+ Revised: 	2026/11/12 23:57:44.349409
// -----------------------------------------------------------------------------

//! Parsing and validating single configuration values. Each error names its variable.

use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use axum::http::HeaderName;
use sqlx::postgres::PgConnectOptions;

use crate::money::Currency;

use super::{ConfigError, DEFAULT_TURNSTILE_VERIFY_URL, TurnstileSettings};

fn invalid(name: &'static str, reason: impl Into<String>) -> ConfigError {
    ConfigError::Invalid { name, reason: reason.into() }
}

pub(super) fn database_url(name: &'static str, value: &str) -> Result<PgConnectOptions, ConfigError> {
    let expected = || invalid(name, "expected postgres://user:password@host:port/database");
    // sqlx accepts any scheme here, so a MySQL or SQLite URL would otherwise get through
    if !["postgres://", "postgresql://"].iter().any(|scheme| value.trim().starts_with(scheme)) {
        return Err(expected());
    }
    // sqlx's own message can quote the URL, password included, so it is not passed on
    value.trim().parse().map_err(|_| expected())
}

pub(super) fn socket_address(name: &'static str, value: &str) -> Result<SocketAddr, ConfigError> {
    value
        .trim()
        .parse()
        .map_err(|_| invalid(name, format!("expected an address and port such as 127.0.0.1:8080, got {value:?}")))
}

pub(super) fn public_url(name: &'static str, value: &str) -> Result<String, ConfigError> {
    let url = value.trim().trim_end_matches('/');
    let host = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(|| invalid(name, "must start with https:// (http:// only for localhost)"))?;
    if host.is_empty() || host.contains(['/', '?', '#', ' ']) {
        return Err(invalid(name, "must be a scheme and host only, e.g. https://licences.example.com"));
    }
    if url.starts_with("http://") && !is_local_host(host) {
        return Err(invalid(name, "must use https:// unless the host is localhost"));
    }
    Ok(url.to_string())
}

/// `off`, or the https:// address the update check asks; answers whether checking is on. Never
/// plain http: the root updater takes the bundle and its checksum from that answer.
pub(super) fn update_source(name: &'static str, value: Option<&str>) -> Result<bool, ConfigError> {
    let Some(value) = value.map(str::trim) else { return Ok(true) };
    if value.eq_ignore_ascii_case("off") {
        return Ok(false);
    }
    match value.strip_prefix("https://") {
        Some(rest) if !rest.is_empty() && !rest.contains(char::is_whitespace) => Ok(true),
        _ => Err(invalid(name, "expected off or an https:// address that answers in GitHub's release format")),
    }
}

/// The Turnstile keys, both or neither, and the address that verifies answers (https://, or
/// plain http:// to this computer, for tests).
pub(super) fn turnstile(
    (site_key_name, site_key): (&'static str, Option<String>),
    (secret_name, secret): (&'static str, Option<String>),
    (url_name, url): (&'static str, Option<String>),
) -> Result<Option<TurnstileSettings>, ConfigError> {
    let verify_url = match url.as_deref().map(str::trim) {
        None => DEFAULT_TURNSTILE_VERIFY_URL.to_string(),
        Some(url) => {
            let host = url.strip_prefix("https://").or_else(|| {
                url.strip_prefix("http://").filter(|rest| is_local_host(rest.split('/').next().unwrap_or_default()))
            });
            match host {
                Some(rest) if !rest.is_empty() && !rest.contains(char::is_whitespace) => url.to_string(),
                _ => return Err(invalid(url_name, "expected an https:// address (http:// only for localhost)")),
            }
        }
    };
    match (site_key, secret) {
        (Some(site_key), Some(secret)) => {
            Ok(Some(TurnstileSettings { site_key: site_key.trim().into(), secret: secret.trim().into(), verify_url }))
        }
        (None, None) => Ok(None),
        (Some(_), None) => Err(invalid(secret_name, format!("must be set when {site_key_name} is"))),
        (None, Some(_)) => Err(invalid(site_key_name, format!("must be set when {secret_name} is"))),
    }
}

fn is_local_host(host_and_port: &str) -> bool {
    let host = host_and_port.rsplit_once(':').map_or(host_and_port, |(host, _port)| host);
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

/// A comma-separated list of IP addresses; an empty list (`none`) trusts no proxy.
pub(super) fn ip_list(name: &'static str, value: &str) -> Result<Vec<IpAddr>, ConfigError> {
    if value.trim().eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(str::trim)
        .map(|item| item.parse().map_err(|_| invalid(name, format!("{item:?} is not an IP address"))))
        .collect()
}

pub(super) fn header_name(name: &'static str, value: &str) -> Result<HeaderName, ConfigError> {
    HeaderName::from_str(value.trim()).map_err(|_| invalid(name, format!("{value:?} is not a header name")))
}

/// [`positive`], at most `max`: for lengths of time, where a huge value would overflow the calendar
/// when added to today.
pub(super) fn span(name: &'static str, value: Option<String>, default: u64, max: u64) -> Result<u64, ConfigError> {
    let number = positive(name, value, default)?;
    if number > max {
        return Err(invalid(name, format!("at most {max} (ten years), got {number}")));
    }
    Ok(number)
}

/// [`span`], at least `min` too.
pub(super) fn span_from(
    name: &'static str,
    value: Option<String>,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, ConfigError> {
    let number = span(name, value, default, max)?;
    if number < min {
        return Err(invalid(name, format!("at least {min}, got {number}")));
    }
    Ok(number)
}

/// A three-letter ISO 4217 code, EUR when unset.
pub(super) fn currency(name: &'static str, value: Option<String>) -> Result<Currency, ConfigError> {
    match value {
        None => Ok(Currency::euro()),
        Some(code) => Currency::parse(code.trim())
            .map_err(|_| invalid(name, format!("expected a three-letter ISO 4217 code such as EUR, got {code:?}"))),
    }
}

/// A whole number above zero, or `default` when unset.
pub(super) fn positive<T>(name: &'static str, value: Option<String>, default: T) -> Result<T, ConfigError>
where
    T: FromStr + PartialOrd + Default,
{
    let Some(value) = value else { return Ok(default) };
    match value.trim().parse::<T>() {
        Ok(number) if number > T::default() => Ok(number),
        _ => Err(invalid(name, format!("expected a whole number above 0, got {value:?}"))),
    }
}
