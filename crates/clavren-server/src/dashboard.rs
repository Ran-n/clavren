// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.251546
//+ Revised: 	2026/11/11 16:32:50.802771
// -----------------------------------------------------------------------------

//! The admin dashboard and the public licence application form, embedded in the binary by
//! `build.rs`: their files by path, `apply.html` for `/apply` and the pages under it, and
//! `index.html` for every other page path so the dashboard's own routes survive a reload. Unknown
//! paths under `/api` and `/health` stay JSON `not_found` errors.

use axum::body::Body;
use axum::http::{HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

use crate::http::error::ApiError;

include!(concat!(env!("OUT_DIR"), "/dashboard_assets.rs"));

const INDEX: &str = "index.html";
const APPLY: &str = "apply.html";
/// Only the dashboard's own files: no inline scripts, nothing from elsewhere, no framing. Images
/// may be `data:` URIs (the authenticator QR code).
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; img-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";
/// The application form also runs Cloudflare Turnstile's script, which shows its check in a frame.
const APPLY_CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self' https://challenges.cloudflare.com; frame-src https://challenges.cloudflare.com; img-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";
/// Vite names built files after their content, so they never change under the same name.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
const NOT_BUILT: &str = "<!doctype html><title>Clavren</title><p>The dashboard isn't built into this binary. Run <code>npm run build</code> in <code>web/admin</code>, then build Clavren again.</p>";

/// The router's fallback: everything no API route matched.
pub async fn serve(method: Method, uri: Uri) -> Response {
    let path = uri.path();
    if path.starts_with("/api/") || path == "/api" || path.starts_with("/health") {
        return ApiError::not_found().into_response();
    }
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let relative = path.trim_start_matches('/');
    if relative == "apply" || relative.starts_with("apply/") {
        return match find(APPLY) {
            Some(form) => page(StatusCode::OK, form, APPLY_CONTENT_SECURITY_POLICY),
            None => page(StatusCode::SERVICE_UNAVAILABLE, NOT_BUILT.as_bytes(), CONTENT_SECURITY_POLICY),
        };
    }
    match find(relative) {
        Some(bytes) => file(relative, bytes),
        // A missing file is a 404; any other path is a page of the dashboard.
        None if relative.rsplit('/').next().is_some_and(|name| name.contains('.')) => {
            StatusCode::NOT_FOUND.into_response()
        }
        None => match find(INDEX) {
            Some(index) => file(INDEX, index),
            None => page(StatusCode::SERVICE_UNAVAILABLE, NOT_BUILT.as_bytes(), CONTENT_SECURITY_POLICY),
        },
    }
}

fn find(path: &str) -> Option<&'static [u8]> {
    let path = if path.is_empty() { INDEX } else { path };
    ASSETS.iter().find(|(name, _)| *name == path).map(|(_, bytes)| *bytes)
}

fn file(path: &str, bytes: &'static [u8]) -> Response {
    let path = if path.is_empty() { INDEX } else { path };
    if path == INDEX {
        return page(StatusCode::OK, bytes, CONTENT_SECURITY_POLICY);
    }
    if path == APPLY {
        return page(StatusCode::OK, bytes, APPLY_CONTENT_SECURITY_POLICY);
    }
    let cache = if path.starts_with("assets/") { IMMUTABLE } else { "no-cache" };
    let mut response = Response::new(Body::from(bytes));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type(path)));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}

/// An HTML page: never cached (it names the current build's files), with the security headers.
fn page(status: StatusCode, bytes: &'static [u8], policy: &'static str) -> Response {
    let mut response = (status, Body::from(bytes)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(policy));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    response
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, extension)| extension) {
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("json" | "webmanifest") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_types_follow_the_extension() {
        assert_eq!(content_type("assets/index-3f2a.js"), "text/javascript; charset=utf-8");
        assert_eq!(content_type("assets/index-3f2a.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("favicon.svg"), "image/svg+xml");
        assert_eq!(content_type("README"), "application/octet-stream");
    }
}
