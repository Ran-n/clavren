// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:30.548240
//+ Revised: 	2026/11/14 11:36:30.548240
// -----------------------------------------------------------------------------

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
        (ErrorCode::ApplicationPending, StatusCode::CONFLICT),
        (ErrorCode::ApplicationUnknown, StatusCode::NOT_FOUND),
        (ErrorCode::HumanCheckFailed, StatusCode::FORBIDDEN),
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
