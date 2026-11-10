// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.829876
//+ Revised: 	2026/11/09 22:30:53.779134
// -----------------------------------------------------------------------------

//! `/api/v1`: the endpoints applications call. Every answer, errors included, carries the server's
//! time. Bodies are limited to 16 KiB; each client address is rate limited, and so are the checks
//! of each activation (counted in [`checks::check`], once its challenge proved it exists).

use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use clavren_protocol::encoding;
use clavren_protocol::wire::{
    ActivationRequest, ActivationResponse, ChallengeResponse, CheckRequest, CheckResponse, ErrorCode,
};
use uuid::Uuid;

use super::{activate, checks};
use crate::http::AppState;
use crate::http::client_ip::{ClientIp, limit_key};
use crate::http::error::ApiError;
use crate::http::extract::{ApiJson, ApiPath};
use crate::http::rate_limit::FailureLimiter;

pub const MAX_BODY_BYTES: usize = 16 * 1024;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/activations", post(activation))
        .route("/activations/{id}/challenges", post(challenge))
        .route("/activations/{id}/checks", post(check))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
}

async fn activation(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    request: Result<ApiJson<ActivationRequest>, ApiError>,
) -> Result<Response, ApiError> {
    let now = state.clock.now();
    let answer = async {
        limit(&state.public_limits.activations, &limit_key(ip), now)?;
        let ApiJson(request) = request?;
        let activated = activate::activate(&state, &request, ip, now).await?;
        let status = if activated.created { StatusCode::CREATED } else { StatusCode::OK };
        if activated.created {
            tracing::info!(activation = %activated.activation_id, %ip, "computer activated");
        }
        let body = ActivationResponse {
            activation: activated.activation_id,
            token: activated.token.token,
            token_expires_at: activated.token.expires_at,
            licence_expires_at: activated.licence_expires_at,
            server_time: now,
        };
        Ok((status, Json(body)).into_response())
    };
    answer.await.map_err(|error| refused(error, "activation", ip, now))
}

async fn challenge(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    path: Result<ApiPath<Uuid>, ApiError>,
) -> Result<Json<ChallengeResponse>, ApiError> {
    let now = state.clock.now();
    let answer = async {
        limit(&state.public_limits.checks_per_ip, &limit_key(ip), now)?;
        let activation = activation_id(path)?;
        let challenge = checks::challenge(&state, activation, now).await?;
        Ok(Json(ChallengeResponse {
            challenge: challenge.id,
            nonce: encoding::encode(&challenge.nonce),
            expires_at: challenge.expires_at,
            server_time: now,
        }))
    };
    answer.await.map_err(|error| refused(error, "challenge", ip, now))
}

async fn check(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    path: Result<ApiPath<Uuid>, ApiError>,
    request: Result<ApiJson<CheckRequest>, ApiError>,
) -> Result<Json<CheckResponse>, ApiError> {
    let now = state.clock.now();
    let answer = async {
        limit(&state.public_limits.checks_per_ip, &limit_key(ip), now)?;
        let activation = activation_id(path)?;
        let ApiJson(request) = request?;
        let checked = checks::check(&state, activation, &request, ip, now).await?;
        Ok(Json(CheckResponse {
            token: checked.token.token,
            token_expires_at: checked.token.expires_at,
            licence_expires_at: checked.licence_expires_at,
            server_time: now,
        }))
    };
    answer.await.map_err(|error| refused(error, "check", ip, now))
}

/// A malformed activation ID can't name an activation.
fn activation_id(path: Result<ApiPath<Uuid>, ApiError>) -> Result<Uuid, ApiError> {
    path.map(|ApiPath(id)| id).map_err(|_| ApiError::refused(ErrorCode::ActivationRevoked, "no such activation"))
}

fn limit(limiter: &FailureLimiter, key: &str, now: DateTime<Utc>) -> Result<(), ApiError> {
    limiter.hit(key, now).map_err(ApiError::rate_limited)
}

/// Logs a refusal (never the key or the token) and adds the server's time.
fn refused(error: ApiError, what: &str, ip: std::net::IpAddr, now: DateTime<Utc>) -> ApiError {
    if error.code() != ErrorCode::InternalError.as_str() {
        tracing::warn!(%ip, code = error.code(), "{what} refused");
    }
    error.at(now)
}
