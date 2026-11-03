// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.037326
//+ Revised: 	2026/11/03 04:54:20.270129
// -----------------------------------------------------------------------------

//! The client's real address. Behind Cloudflare Tunnel every connection comes from `cloudflared`
//! on localhost, and the real address is in a header; that header is believed only when the
//! connection comes from a trusted proxy, otherwise anyone could pick their own address and dodge
//! the rate limits.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::request::Parts;

use super::AppState;
use super::error::ApiError;
use crate::config::ProxySettings;

pub fn resolve(peer: IpAddr, headers: &HeaderMap, proxies: &ProxySettings) -> IpAddr {
    if !proxies.trusted.contains(&peer) {
        return peer;
    }
    headers
        .get(&proxies.client_ip_header)
        .and_then(|value| value.to_str().ok())
        // X-Forwarded-For style lists: the first entry is the original client
        .and_then(|value| value.split(',').next())
        .and_then(|first| first.trim().parse().ok())
        .unwrap_or(peer)
}

/// Extracts the resolved client address.
pub struct ClientIp(pub IpAddr);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        // the extractor, not the raw extension, so tests can supply the address with `MockConnectInfo`
        let ConnectInfo(peer) = ConnectInfo::<SocketAddr>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::internal("the connection's address is unknown"))?;
        Ok(Self(resolve(peer.ip(), &parts.headers, &state.settings.proxies)))
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderName, HeaderValue};

    use super::*;

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(HeaderName::from_static("cf-connecting-ip"), HeaderValue::from_str(value).unwrap());
        headers
    }

    fn localhost() -> IpAddr {
        "127.0.0.1".parse().unwrap()
    }

    #[test]
    fn a_trusted_proxy_passes_the_client_address_on() {
        let resolved = resolve(localhost(), &headers("203.0.113.7"), &ProxySettings::default());
        assert_eq!(resolved, "203.0.113.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn the_header_from_anyone_else_is_ignored() {
        let stranger: IpAddr = "198.51.100.4".parse().unwrap();
        assert_eq!(resolve(stranger, &headers("203.0.113.7"), &ProxySettings::default()), stranger);
    }

    #[test]
    fn a_missing_or_garbled_header_falls_back_to_the_peer() {
        assert_eq!(resolve(localhost(), &HeaderMap::new(), &ProxySettings::default()), localhost());
        assert_eq!(resolve(localhost(), &headers("not-an-ip"), &ProxySettings::default()), localhost());
    }

    #[test]
    fn a_forwarded_list_uses_its_first_entry() {
        let resolved = resolve(localhost(), &headers("2001:db8::1, 10.0.0.1"), &ProxySettings::default());
        assert_eq!(resolved, "2001:db8::1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn trusting_nobody_always_uses_the_peer() {
        let settings = ProxySettings { trusted: Vec::new(), ..ProxySettings::default() };
        assert_eq!(resolve(localhost(), &headers("203.0.113.7"), &settings), localhost());
    }
}
