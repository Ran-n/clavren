// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.848432
//+ Revised: 	2026/11/03 04:54:19.272808
// -----------------------------------------------------------------------------

//! Parsing and validating single configuration values. Each error names its variable.

use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use axum::http::HeaderName;
use sqlx::postgres::PgConnectOptions;

use super::ConfigError;

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
