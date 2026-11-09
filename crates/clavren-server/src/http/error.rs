// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.097530
//+ Revised: 	2026/11/09 10:00:41.925039
// -----------------------------------------------------------------------------

//! Every error the API returns: `{ "error": "<code>", "message": "<English, for logs>" }` with the
//! matching status, plus `Retry-After` when rate limited. Applications and the dashboard decide
//! what to show from the code; the message is never meant for end users.

use std::fmt::Display;
use std::time::Duration;

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use clavren_protocol::wire::ErrorCode;
use serde_json::json;
use uuid::Uuid;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    retry_after: Option<Duration>,
    /// For bulk requests: the records that blocked the whole batch.
    ids: Option<Vec<Uuid>>,
    /// For the public API, whose every answer carries the server's time.
    server_time: Option<DateTime<Utc>>,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into(), retry_after: None, ids: None, server_time: None }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    /// Wrong username, password, or code: one answer for all, so a failure never says which.
    pub fn invalid_credentials() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "invalid_credentials", "the username, password or code is wrong")
    }

    /// No session, or it expired, was revoked, or its admin was disabled.
    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "sign in to continue")
    }

    pub fn csrf_failed() -> Self {
        Self::new(StatusCode::FORBIDDEN, "csrf_failed", "the request's CSRF token is missing or wrong")
    }

    /// A refusal of the public API, with its documented code.
    pub fn refused(code: ErrorCode, message: impl Into<String>) -> Self {
        let status = match code {
            ErrorCode::InvalidKey => StatusCode::NOT_FOUND,
            ErrorCode::AlreadyActivated => StatusCode::CONFLICT,
            ErrorCode::ChallengeInvalid => StatusCode::UNAUTHORIZED,
            ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::InvalidRequest => StatusCode::BAD_REQUEST,
            ErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::LicenceRevoked
            | ErrorCode::LicenceExpired
            | ErrorCode::ProductUnavailable
            | ErrorCode::ActivationRevoked => StatusCode::FORBIDDEN,
        };
        Self::new(status, code.as_str(), message)
    }

    /// Adds the server's time, as every public API answer has.
    pub fn at(self, server_time: DateTime<Utc>) -> Self {
        Self { server_time: Some(server_time), ..self }
    }

    /// Adds the IDs of the records that blocked a bulk request; the answer lists them in `ids`.
    pub fn with_ids(self, ids: Vec<Uuid>) -> Self {
        Self { ids: Some(ids), ..self }
    }

    /// The resource doesn't exist (or the ID in the path is malformed).
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "there is nothing at this address")
    }

    /// The request clashes with the current data; `code` says how, e.g. `product_code_taken`.
    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    /// A file the update scripts wrote can't be read; `message` says what to run on the server.
    pub fn update_file_unreadable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "update_file_unreadable", message)
    }

    pub fn weak_password(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "weak_password", message)
    }

    pub fn rate_limited(retry_after: Duration) -> Self {
        let mut error = Self::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", "too many attempts; try again later");
        error.retry_after = Some(retry_after);
        error
    }

    /// An unexpected failure: logged in full here, answered with nothing but the code.
    pub fn internal(error: impl Display) -> Self {
        tracing::error!(%error, "request failed");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "something went wrong on the server")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut body = json!({ "error": self.code, "message": self.message });
        if let Some(ids) = self.ids {
            body["ids"] = json!(ids);
        }
        if let Some(server_time) = self.server_time {
            body["server_time"] = json!(server_time);
        }
        let mut response = (self.status, Json(body)).into_response();
        if let Some(retry_after) = self.retry_after {
            // whole seconds, rounded up, so a client never retries a moment too early
            let seconds = retry_after.as_secs() + u64::from(retry_after.subsec_nanos() > 0);
            response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(seconds.max(1)));
        }
        response
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::internal(format!("database: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

    use super::*;

    async fn body(error: ApiError) -> (StatusCode, Option<String>, String) {
        let response = error.into_response();
        let status = response.status();
        let retry = response.headers().get(header::RETRY_AFTER).map(|value| value.to_str().unwrap().to_string());
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        (status, retry, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn errors_have_the_documented_shape() {
        let (status, retry, text) = body(ApiError::invalid_credentials()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(retry, None);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["error"], "invalid_credentials");
        assert!(parsed["message"].is_string());
        assert_eq!(parsed.as_object().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn bulk_errors_name_the_blocking_records() {
        let id = Uuid::now_v7();
        let (status, _, text) = body(ApiError::not_found().with_ids(vec![id])).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["ids"], json!([id]));
    }

    #[tokio::test]
    async fn public_refusals_carry_their_status_and_the_server_time() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        for (code, status) in [
            (ErrorCode::InvalidKey, StatusCode::NOT_FOUND),
            (ErrorCode::AlreadyActivated, StatusCode::CONFLICT),
            (ErrorCode::ChallengeInvalid, StatusCode::UNAUTHORIZED),
            (ErrorCode::LicenceExpired, StatusCode::FORBIDDEN),
            (ErrorCode::RateLimited, StatusCode::TOO_MANY_REQUESTS),
            (ErrorCode::InvalidRequest, StatusCode::BAD_REQUEST),
            (ErrorCode::InternalError, StatusCode::INTERNAL_SERVER_ERROR),
        ] {
            let (answered, _, text) = body(ApiError::refused(code, "no").at(now)).await;
            let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!((answered, parsed["error"].as_str()), (status, Some(code.as_str())));
            assert_eq!(parsed["server_time"], json!(now));
        }
    }

    #[tokio::test]
    async fn rate_limits_round_retry_after_up_to_whole_seconds() {
        let (status, retry, _) = body(ApiError::rate_limited(Duration::from_millis(61_200))).await;
        assert_eq!((status, retry.as_deref()), (StatusCode::TOO_MANY_REQUESTS, Some("62")));
        let (_, retry, _) = body(ApiError::rate_limited(Duration::ZERO)).await;
        assert_eq!(retry.as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn internal_errors_reveal_nothing() {
        let (status, _, text) = body(ApiError::internal("relation \"admin_users\" does not exist")).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!text.contains("admin_users"), "{text}");
    }
}
