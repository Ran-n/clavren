// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.794899
//+ Revised: 	2026/11/09 10:00:41.870192
// -----------------------------------------------------------------------------

//! The HTTP side: the router, its shared state and the layers every request goes through.

pub mod client_ip;
pub mod error;
pub mod expand;
pub mod extract;
pub mod fields;
pub mod health;
pub mod pagination;
pub mod patch;
pub mod rate_limit;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Request, StatusCode, header};
use sqlx::PgPool;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing::Span;

use crate::clock::Clock;
use crate::config::{AdminAuthSettings, Config, LicensingSettings, ProxySettings, UpdateSettings};
use crate::crypto::key_ring::KeyRing;
use crate::{
    activations, admin_auth, admin_preferences, audit, backups, customers, dashboard, licences, overview, products,
    updates,
};
use rate_limit::{FailureLimiter, PublicLimits};

/// A request that takes longer is answered with 408, so shutdown never waits on it for long.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub keys: Arc<KeyRing>,
    pub clock: Arc<dyn Clock>,
    pub settings: Arc<Settings>,
    /// Failed admin logins, per client address and per username.
    pub login_failures: Arc<FailureLimiter>,
    pub public_limits: Arc<PublicLimits>,
}

/// The parts of the configuration request handlers need.
#[derive(Debug, Clone)]
pub struct Settings {
    pub public_url: String,
    pub proxies: ProxySettings,
    pub admin: AdminAuthSettings,
    pub licensing: LicensingSettings,
    pub updates: UpdateSettings,
}

impl AppState {
    pub fn new(pool: PgPool, keys: KeyRing, clock: Arc<dyn Clock>, settings: Settings) -> Self {
        let login_failures =
            FailureLimiter::new(settings.admin.login_max_failures, settings.admin.login_failure_window);
        Self {
            pool,
            keys: Arc::new(keys),
            clock,
            settings: Arc::new(settings),
            login_failures: Arc::new(login_failures),
            public_limits: Arc::new(PublicLimits::default()),
        }
    }
}

impl From<&Config> for Settings {
    fn from(config: &Config) -> Self {
        Self {
            public_url: config.public_url.clone(),
            proxies: config.proxies.clone(),
            admin: config.admin.clone(),
            licensing: config.licensing.clone(),
            updates: config.updates.clone(),
        }
    }
}

pub fn router(state: AppState) -> Router {
    let request_id = HeaderName::from_static(REQUEST_ID_HEADER);
    Router::new()
        .merge(health::routes())
        .nest("/api/admin", admin_routes())
        .nest("/api/v1", activations::public_routes::routes().layer(no_store()))
        .fallback(dashboard::serve)
        .with_state(state)
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, REQUEST_TIMEOUT))
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(TraceLayer::new_for_http().make_span_with(request_span))
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
}

/// Everything under `/api/admin`. No response may be cached anywhere: they hold CSRF tokens,
/// recovery codes, licence keys and customers' personal data.
fn admin_routes() -> Router<AppState> {
    admin_auth::routes()
        .merge(products::routes())
        .merge(customers::routes())
        .merge(licences::routes())
        .merge(activations::routes())
        .merge(audit::routes())
        .merge(backups::routes())
        .merge(overview::routes())
        .merge(admin_preferences::routes())
        .merge(updates::routes())
        .layer(no_store())
}

/// Nothing may be cached: answers hold tokens, keys, CSRF tokens and personal data.
fn no_store() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))
}

/// Every line a request logs carries its ID. Only the path is recorded, never the query string:
/// an admin search can put a licence key or a customer's email in it.
fn request_span(request: &Request<Body>) -> Span {
    let request_id = request.headers().get(REQUEST_ID_HEADER).and_then(|value| value.to_str().ok()).unwrap_or("-");
    tracing::info_span!("request", method = %request.method(), path = %request.uri().path(), request_id)
}
