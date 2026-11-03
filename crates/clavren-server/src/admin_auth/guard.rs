// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.016678
//+ Revised: 	2026/11/03 04:54:18.047470
// -----------------------------------------------------------------------------

//! The signed-in admin, for every admin endpoint except login: a valid session cookie, and for
//! anything that changes state, the matching `X-CSRF-Token` header.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::TimeDelta;

use super::cookie;
use super::sessions::{self, ActiveSession, TOUCH_INTERVAL_SECONDS};
use crate::audit::Actor;
use crate::http::AppState;
use crate::http::client_ip::ClientIp;
use crate::http::error::ApiError;

pub const CSRF_HEADER: &str = "x-csrf-token";

pub struct SignedIn {
    pub session: ActiveSession,
    /// The raw session token, to derive the CSRF token from.
    pub token: String,
}

impl FromRequestParts<AppState> for SignedIn {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = cookie::session_token(&parts.headers).ok_or_else(ApiError::unauthorized)?;
        let now = state.clock.now();
        let session = sessions::find_active(&state.pool, &token, now, &state.settings.admin)
            .await?
            .ok_or_else(ApiError::unauthorized)?;
        if !parts.method.is_safe() {
            let provided = parts.headers.get(CSRF_HEADER).and_then(|value| value.to_str().ok()).unwrap_or_default();
            if !sessions::csrf_matches(&state.keys, &token, provided) {
                tracing::warn!(admin = %session.username, "request refused: missing or wrong CSRF token");
                return Err(ApiError::csrf_failed());
            }
        }
        if now - session.last_seen_at > TimeDelta::seconds(TOUCH_INTERVAL_SECONDS) {
            sessions::touch(&state.pool, session.session_id, now).await?;
        }
        Ok(Self { session, token })
    }
}

/// A signed-in admin making a request, as the audit log records them. Every admin endpoint
/// outside sign-in takes this.
pub struct Admin {
    pub actor: Actor,
    pub username: String,
}

impl FromRequestParts<AppState> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let signed_in = SignedIn::from_request_parts(parts, state).await?;
        let ClientIp(ip) = ClientIp::from_request_parts(parts, state).await?;
        let actor = Actor { admin_id: signed_in.session.admin_user_id, ip, at: state.clock.now() };
        Ok(Self { actor, username: signed_in.session.username })
    }
}
