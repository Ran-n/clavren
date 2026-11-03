// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.155294
//+ Revised: 	2026/11/03 04:54:20.437716
// -----------------------------------------------------------------------------

//! Extractors whose rejections use the API's error format instead of axum's plain text: JSON
//! bodies, query strings and path parameters.

use axum::Json;
use axum::extract::{FromRequest, FromRequestParts, Path, Request};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use super::error::ApiError;

pub struct ApiJson<T>(pub T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|rejection| ApiError::invalid_request(rejection.body_text()))
    }
}

/// The query string, apart from `expand` (see [`super::expand::Expand`]), which may repeat.
pub struct ApiQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for ApiQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or_default();
        let without_expand: String = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(form_urlencoded::parse(query.as_bytes()).filter(|(name, _)| name != "expand"))
            .finish();
        serde_urlencoded::from_str(&without_expand)
            .map(Self)
            .map_err(|error| ApiError::invalid_request(format!("invalid query string: {error}")))
    }
}

/// A path parameter. A malformed ID can't name anything, so it is answered as not found.
pub struct ApiPath<T>(pub T);

impl<T, S> FromRequestParts<S> for ApiPath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|Path(value)| Self(value))
            .map_err(|_| ApiError::not_found())
    }
}
