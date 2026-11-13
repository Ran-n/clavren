// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:48.815352
//+ Revised: 	2026/11/12 23:57:44.183609
// -----------------------------------------------------------------------------

//! `/api/v1/applications`: what the form offers, a product's key page, sending an application,
//! and asking how it went.
//! Like the rest of `/api/v1`, every answer carries the server's time and nothing is cached.
//! Sending is limited per client address, and so is asking; the name and email are never logged.

use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use clavren_protocol::wire::{
    ApplicationCreated, ApplicationFormResponse, ApplicationProduct, ApplicationRequest, ApplicationStatusRequest,
    ApplicationStatusResponse, ErrorCode, KeyPageResponse,
};

use super::submit;
use crate::activations::public_routes::MAX_BODY_BYTES;
use crate::http::AppState;
use crate::http::client_ip::{ClientIp, limit_key};
use crate::http::error::ApiError;
use crate::http::extract::ApiJson;
use crate::http::rate_limit::FailureLimiter;
use crate::products;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/applications/form", get(form))
        .route("/applications/products/{code}", get(key_page))
        .route("/applications", post(create))
        .route("/applications/status", post(status))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
}

async fn form(State(state): State<AppState>) -> Result<Json<ApplicationFormResponse>, ApiError> {
    let now = state.clock.now();
    let products = products::repository::accepting_applications(&state.pool)
        .await
        .map_err(|error| ApiError::from(error).at(now))?
        .into_iter()
        .map(|product| ApplicationProduct { code: product.code, name: product.name, description: product.description })
        .collect();
    let turnstile_site_key = state.settings.applications.turnstile.as_ref().map(|turnstile| turnstile.site_key.clone());
    Ok(Json(ApplicationFormResponse { products, turnstile_site_key, server_time: now }))
}

/// A product's key page: how to get a key, and whether it takes applications. Unknown and
/// inactive products get the same 404, as keys never tell products apart. One indexed read and no
/// secrets, so, like the form, it has no rate limit.
async fn key_page(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Path(code): Path<String>,
) -> Result<Json<KeyPageResponse>, ApiError> {
    let now = state.clock.now();
    let answer = async {
        let product = products::repository::find_active_by_code(&state.pool, &code)
            .await?
            .ok_or_else(ApiError::product_not_found)?;
        Ok(Json(KeyPageResponse {
            code: product.code,
            name: product.name,
            description: product.description,
            key_instructions: product.key_instructions,
            accepts_applications: product.accepts_applications,
            server_time: now,
        }))
    };
    answer.await.map_err(|error| refused(error, "key page", ip, now))
}

async fn create(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    request: Result<ApiJson<ApplicationRequest>, ApiError>,
) -> Result<(StatusCode, Json<ApplicationCreated>), ApiError> {
    let now = state.clock.now();
    let answer = async {
        limit(&state.public_limits.applications, ip, now)?;
        let ApiJson(request) = request?;
        let sent = submit::submit(&state, &request, ip, now).await?;
        tracing::info!(application = %sent.id, source = request.source.as_str(), %ip, "licence application received");
        Ok((
            StatusCode::CREATED,
            Json(ApplicationCreated { application: sent.id, secret: sent.secret, server_time: now }),
        ))
    };
    answer.await.map_err(|error| refused(error, "application", ip, now))
}

async fn status(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    request: Result<ApiJson<ApplicationStatusRequest>, ApiError>,
) -> Result<Json<ApplicationStatusResponse>, ApiError> {
    let now = state.clock.now();
    let answer = async {
        limit(&state.public_limits.application_status, ip, now)?;
        let ApiJson(request) = request?;
        let status = submit::status(&state, &request.secret, ip, now).await?;
        Ok(Json(ApplicationStatusResponse {
            application: status.application.id,
            product: status.product_code,
            status: status.status,
            created_at: status.application.created_at,
            decided_at: status.application.decided_at,
            reason: status.application.rejection_reason,
            key: status.key,
            licence_expires_at: status.licence_expires_at,
            server_time: now,
        }))
    };
    answer.await.map_err(|error| refused(error, "application status", ip, now))
}

fn limit(limiter: &FailureLimiter, ip: std::net::IpAddr, now: DateTime<Utc>) -> Result<(), ApiError> {
    limiter.hit(&limit_key(ip), now).map_err(ApiError::rate_limited)
}

/// Logs a refusal (never the secret, the name or the email) and adds the server's time.
fn refused(error: ApiError, what: &str, ip: std::net::IpAddr, now: DateTime<Utc>) -> ApiError {
    if error.code() != ErrorCode::InternalError.as_str() {
        tracing::warn!(%ip, code = error.code(), "{what} refused");
    }
    error.at(now)
}
