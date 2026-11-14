// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:59:04.335065
//+ Revised: 	2026/11/14 11:59:04.335065
// -----------------------------------------------------------------------------

//! The checks of a product's fields that have rules of their own: its code, message template,
//! key instructions and offline days.

use super::{MAX_OFFLINE_DAYS, MIN_OFFLINE_DAYS, message_template};
use crate::http::error::ApiError;
use crate::http::fields;
use serde::Deserialize;

pub(super) const MAX_KEY_INSTRUCTIONS: usize = 2000;

pub(super) const MAX_TEMPLATE: usize = 10_000;

pub(super) const MAX_CODE: usize = 63;

/// A whole number when the field is there: `null` is refused, not taken as "unchanged".
pub(super) fn whole_days<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<i32>, D::Error> {
    i32::deserialize(deserializer).map(Some)
}

/// Within [`MIN_OFFLINE_DAYS`] and [`MAX_OFFLINE_DAYS`].
pub(super) fn offline_days(days: i32) -> Result<i32, ApiError> {
    if (MIN_OFFLINE_DAYS..=MAX_OFFLINE_DAYS).contains(&days) {
        Ok(days)
    } else {
        Err(ApiError::invalid_request(format!(
            "offline_days must be from {MIN_OFFLINE_DAYS} to {MAX_OFFLINE_DAYS} days, got {days}"
        )))
    }
}

/// The instructions trimmed, blank as none, at most 2,000 characters.
pub(super) fn key_instructions(value: Option<&str>) -> Result<Option<String>, ApiError> {
    fields::optional("key_instructions", value, MAX_KEY_INSTRUCTIONS)
}

/// Another product already has the code, typed or made.
pub(super) fn code_taken_error() -> ApiError {
    ApiError::conflict("product_code_taken", "another product already has this code")
}

/// Lowercase letters, digits and `-`, starting with a letter or digit; typed in any case.
pub(super) fn code(value: &str) -> Result<String, ApiError> {
    let code = value.trim().to_ascii_lowercase();
    let mut characters = code.chars();
    let valid = characters.next().is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && characters.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && code.len() <= MAX_CODE;
    if valid {
        Ok(code)
    } else {
        Err(ApiError::invalid_request(
            "code must be up to 63 lowercase letters, digits and dashes, starting with a letter or digit",
        ))
    }
}

pub(super) fn template_text(value: &str) -> Result<String, ApiError> {
    if value.chars().count() > MAX_TEMPLATE {
        return Err(ApiError::invalid_request(format!("message_template must be at most {MAX_TEMPLATE} characters")));
    }
    match message_template::unknown_placeholder(value) {
        Some(name) => Err(ApiError::invalid_request(format!(
            "message_template uses {{{name}}}; the placeholders are {{customer_name}}, {{product_name}} and {{key}}"
        ))),
        None => Ok(value.to_string()),
    }
}
