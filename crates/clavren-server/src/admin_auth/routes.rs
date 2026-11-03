// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.373694
//+ Revised: 	2026/11/03 04:54:18.523965
// -----------------------------------------------------------------------------

//! The admin authentication endpoints, under `/api/admin`.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{post, put};
use chrono::{DateTime, Utc};
use qrcode::QrCode;
use qrcode::render::svg;
use serde_json::json;
use uuid::Uuid;

use super::account::{self, ChangePassword, ConfirmIdentity, ReplaceAuthenticator};
use super::guard::SignedIn;
use super::login::{self, LoginRequest};
use super::{cookie, sessions};
use crate::audit::{self, AuditEntry};
use crate::http::AppState;
use crate::http::client_ip::ClientIp;
use crate::http::error::ApiError;
use crate::http::extract::ApiJson;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/session", post(sign_in).get(current).delete(sign_out))
        .route("/session/revoke-others", post(revoke_other_sessions))
        .route("/account/password", post(change_password))
        .route("/account/recovery-codes", post(regenerate_recovery_codes))
        .route("/account/totp/secret", post(new_authenticator_secret))
        .route("/account/totp", put(replace_authenticator))
}

fn session_body(
    state: &AppState,
    admin_id: Uuid,
    username: &str,
    token: &str,
    expires_at: DateTime<Utc>,
) -> Json<serde_json::Value> {
    Json(json!({
        "admin": { "id": admin_id, "username": username },
        "csrf_token": sessions::csrf_token(&state.keys, token),
        "expires_at": expires_at,
    }))
}

async fn sign_in(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ApiJson(request): ApiJson<LoginRequest>,
) -> Result<Response, ApiError> {
    let signed_in = login::login(&state, request, ip).await?;
    let cookie = cookie::set(
        &signed_in.session.token,
        state.settings.admin.session_lifetime,
        state.settings.admin.secure_cookies,
    );
    let body =
        session_body(&state, signed_in.id, &signed_in.username, &signed_in.session.token, signed_in.session.expires_at);
    Ok(([(header::SET_COOKIE, cookie)], body).into_response())
}

async fn current(State(state): State<AppState>, signed_in: SignedIn) -> Json<serde_json::Value> {
    let session = &signed_in.session;
    session_body(&state, session.admin_user_id, &session.username, &signed_in.token, session.expires_at)
}

async fn sign_out(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    signed_in: SignedIn,
) -> Result<Response, ApiError> {
    let now = state.clock.now();
    let session = &signed_in.session;
    let mut transaction = state.pool.begin().await?;
    sessions::revoke(&mut *transaction, session.session_id, now).await?;
    let entry = AuditEntry::admin_account(
        "admin.logout",
        Some(session.admin_user_id),
        Some(session.admin_user_id),
        Some(ip),
        now,
    );
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    tracing::info!(admin = %session.username, "admin signed out");
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, cookie::clear(state.settings.admin.secure_cookies))])
        .into_response())
}

async fn revoke_other_sessions(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    signed_in: SignedIn,
) -> Result<Json<serde_json::Value>, ApiError> {
    let now = state.clock.now();
    let session = &signed_in.session;
    let mut transaction = state.pool.begin().await?;
    let revoked = sessions::revoke_all(&mut *transaction, session.admin_user_id, Some(session.session_id), now).await?;
    let entry = AuditEntry::admin_account(
        "admin.sessions_revoked",
        Some(session.admin_user_id),
        Some(session.admin_user_id),
        Some(ip),
        now,
    )
    .with_metadata(json!({ "count": revoked }));
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    tracing::info!(admin = %session.username, revoked, "admin signed out their other sessions");
    Ok(Json(json!({ "revoked": revoked })))
}

async fn change_password(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    signed_in: SignedIn,
    ApiJson(request): ApiJson<ChangePassword>,
) -> Result<StatusCode, ApiError> {
    account::change_password(&state, &signed_in, request, ip).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn regenerate_recovery_codes(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    signed_in: SignedIn,
    ApiJson(request): ApiJson<ConfirmIdentity>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let codes = account::regenerate_recovery_codes(&state, &signed_in, request, ip).await?;
    Ok(Json(json!({ "recovery_codes": codes })))
}

async fn new_authenticator_secret(signed_in: SignedIn) -> Result<Json<serde_json::Value>, ApiError> {
    let fresh = account::new_authenticator_secret(&signed_in)?;
    let qr_svg = qr_svg(&fresh.uri)?;
    Ok(Json(json!({ "secret": fresh.secret, "uri": fresh.uri, "qr_svg": qr_svg })))
}

/// The setup URI as an SVG QR code, for the dashboard to show without a QR library of its own.
fn qr_svg(uri: &str) -> Result<String, ApiError> {
    let code = QrCode::new(uri.as_bytes()).map_err(ApiError::internal)?;
    Ok(code.render::<svg::Color<'_>>().min_dimensions(200, 200).build())
}

async fn replace_authenticator(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    signed_in: SignedIn,
    ApiJson(request): ApiJson<ReplaceAuthenticator>,
) -> Result<StatusCode, ApiError> {
    account::replace_authenticator(&state, &signed_in, request, ip).await?;
    Ok(StatusCode::NO_CONTENT)
}
