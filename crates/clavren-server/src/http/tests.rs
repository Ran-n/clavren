// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:31.128599
//+ Revised: 	2026/11/14 11:36:31.128599
// -----------------------------------------------------------------------------

use axum::body::to_bytes;
use axum::http::StatusCode;
use axum::routing::get;
use tower::ServiceExt;

use super::*;

/// Regression: tower-http's timeout answered 408 with an empty body, the one error without
/// the documented shape.
#[tokio::test]
async fn a_request_that_takes_too_long_gets_408_in_the_error_shape() {
    let slow = || async {
        tokio::time::sleep(Duration::from_secs(60)).await;
        "late"
    };
    let app = Router::new()
        .route("/slow", get(slow))
        .route("/quick", get(|| async { "in time" }))
        .layer(middleware::from_fn(|request, next| time_limited(Duration::from_millis(20), request, next)));
    let ask = |path: &'static str| app.clone().oneshot(Request::get(path).body(Body::empty()).unwrap());

    let late = ask("/slow").await.unwrap();
    assert_eq!(late.status(), StatusCode::REQUEST_TIMEOUT);
    let body: serde_json::Value = serde_json::from_slice(&to_bytes(late.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(body["error"], "request_timeout");
    assert!(body["message"].is_string());
    assert_eq!(ask("/quick").await.unwrap().status(), StatusCode::OK);
}
