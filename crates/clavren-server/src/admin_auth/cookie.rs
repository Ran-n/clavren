// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:34.886919
//+ Revised: 	2026/11/03 04:54:17.927402
// -----------------------------------------------------------------------------

//! The session cookie: `HttpOnly` (no script can read it), `SameSite=Strict` (never sent from
//! another site), `Secure` over HTTPS, and scoped to the whole site.

use std::time::Duration;

use axum::http::{HeaderMap, HeaderValue, header};

pub const SESSION_COOKIE: &str = "clavren_session";

pub fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.to_string())
        .filter(|value| !value.is_empty())
}

pub fn set(token: &str, max_age: Duration, secure: bool) -> HeaderValue {
    cookie(token, max_age.as_secs(), secure)
}

pub fn clear(secure: bool) -> HeaderValue {
    cookie("", 0, secure)
}

fn cookie(value: &str, max_age_seconds: u64, secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    let text =
        format!("{SESSION_COOKIE}={value}; Path=/; Max-Age={max_age_seconds}; HttpOnly; SameSite=Strict{secure}");
    // the token is base64url, so the value is always a valid header
    HeaderValue::from_str(&text).unwrap_or_else(|_| HeaderValue::from_static("clavren_session=; Max-Age=0"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_cookie(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn the_session_token_is_found_among_other_cookies() {
        assert_eq!(session_token(&with_cookie("theme=dark; clavren_session=abc_-1; x=y")).as_deref(), Some("abc_-1"));
        assert_eq!(session_token(&with_cookie("clavren_session=")), None);
        assert_eq!(session_token(&with_cookie("other_clavren_session=abc")), None);
        assert_eq!(session_token(&HeaderMap::new()), None);
    }

    #[test]
    fn the_cookie_has_every_protection() {
        let value = set("token", Duration::from_secs(604_800), true);
        assert_eq!(value, "clavren_session=token; Path=/; Max-Age=604800; HttpOnly; SameSite=Strict; Secure");
        assert_eq!(
            set("token", Duration::from_secs(60), false),
            "clavren_session=token; Path=/; Max-Age=60; HttpOnly; SameSite=Strict"
        );
    }

    #[test]
    fn clearing_expires_the_cookie_at_once() {
        assert_eq!(clear(true), "clavren_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict; Secure");
    }
}
