// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.037326
//+ Revised: 	2026/11/09 16:05:47.686164
// -----------------------------------------------------------------------------

//! The client's real address. Behind Cloudflare Tunnel every connection comes from `cloudflared`
//! on localhost, and the real address is in a header; that header is believed only when the
//! connection comes from a trusted proxy, otherwise anyone could pick their own address and dodge
//! the rate limits.

use std::net::{IpAddr, Ipv6Addr, SocketAddr};

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
        // X-Forwarded-For style lists: the proxy appends the address it saw, so the last entry is
        // the only one it vouches for; the ones before it are whatever the client sent
        .and_then(|value| value.rsplit(',').next())
        .and_then(|last| last.trim().parse().ok())
        .unwrap_or(peer)
}

/// The rate-limit key for a client address. An IPv6 address counts by its /64 network: one
/// connection usually gets a whole /64, so counting single addresses would let it pick a new one
/// for every request.
pub fn limit_key(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => format!("ip:{ip}"),
        IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
            Some(ip) => format!("ip:{ip}"),
            None => {
                let network = u128::from(ip) & !((1u128 << 64) - 1);
                format!("ip:{}/64", Ipv6Addr::from(network))
            }
        },
    }
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

    /// Regression: the first entry was used, which the client writes itself when the proxy
    /// appends to the list; anyone could pick their own address and dodge the limits.
    #[test]
    fn a_forwarded_list_uses_the_entry_the_proxy_added() {
        let resolved = resolve(localhost(), &headers("192.0.2.1, 2001:db8::1"), &ProxySettings::default());
        assert_eq!(resolved, "2001:db8::1".parse::<IpAddr>().unwrap());
        let resolved = resolve(localhost(), &headers("203.0.113.9,198.51.100.3"), &ProxySettings::default());
        assert_eq!(resolved, "198.51.100.3".parse::<IpAddr>().unwrap());
    }

    /// Regression: IPv6 addresses were counted one by one, so a single /64 had 2^64 allowances.
    #[test]
    fn ipv6_addresses_count_by_their_network() {
        let key = |text: &str| limit_key(text.parse().unwrap());
        assert_eq!(key("2001:db8:1:2:aaaa::1"), key("2001:db8:1:2:ffff:ffff:ffff:ffff"));
        assert_eq!(key("2001:db8:1:2::1"), "ip:2001:db8:1:2::/64");
        assert_ne!(key("2001:db8:1:2::1"), key("2001:db8:1:3::1"));
        assert_eq!(key("203.0.113.7"), "ip:203.0.113.7");
        assert_eq!(key("::ffff:203.0.113.7"), "ip:203.0.113.7", "a mapped IPv4 address is that address");
        assert_ne!(key("203.0.113.7"), key("203.0.113.8"));
    }

    #[test]
    fn trusting_nobody_always_uses_the_peer() {
        let settings = ProxySettings { trusted: Vec::new(), ..ProxySettings::default() };
        assert_eq!(resolve(localhost(), &headers("203.0.113.7"), &settings), localhost());
    }
}
