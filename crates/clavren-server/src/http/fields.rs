// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:13.708171
//+ Revised: 	2026/11/03 04:54:13.708171
// -----------------------------------------------------------------------------

//! Checks for text fields from request bodies. Values are trimmed; an optional field left blank
//! counts as empty. Each error names the field, so the dashboard can show it next to the input.

use super::error::ApiError;

pub fn required(field: &str, value: &str, max_chars: usize) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::invalid_request(format!("{field} is required")));
    }
    within(field, value, max_chars)
}

pub fn optional(field: &str, value: Option<&str>, max_chars: usize) -> Result<Option<String>, ApiError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => within(field, value, max_chars).map(Some),
        None => Ok(None),
    }
}

/// A plausible address: something, `@`, a domain with a dot, no spaces. Whether it receives mail
/// is the administrator's business.
pub fn email(value: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(email) = optional("email", value, 320)? else { return Ok(None) };
    let plausible = email.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
    }) && !email.chars().any(char::is_whitespace)
        && email.matches('@').count() == 1;
    if plausible { Ok(Some(email)) } else { Err(ApiError::invalid_request("email is not a valid address")) }
}

fn within(field: &str, value: &str, max_chars: usize) -> Result<String, ApiError> {
    if value.chars().count() > max_chars {
        return Err(ApiError::invalid_request(format!("{field} must be at most {max_chars} characters")));
    }
    Ok(value.to_string())
}

/// `value` as a pattern for `ILIKE '%…%'`, with its own `%`, `_` and `\` matched literally.
pub fn contains_pattern(value: &str) -> String {
    let escaped = value.trim().replace('\\', r"\\").replace('%', r"\%").replace('_', r"\_");
    format!("%{escaped}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_text_is_trimmed_and_bounded() {
        assert_eq!(required("name", "  Ana  ", 10).unwrap(), "Ana");
        assert_eq!(required("name", " ", 10).unwrap_err().code(), "invalid_request");
        assert_eq!(required("name", "ñññ", 3).unwrap(), "ñññ", "characters, not bytes");
        assert!(required("name", "abcd", 3).is_err());
    }

    #[test]
    fn optional_text_treats_blank_as_empty() {
        assert_eq!(optional("notes", None, 5).unwrap(), None);
        assert_eq!(optional("notes", Some("  "), 5).unwrap(), None);
        assert_eq!(optional("notes", Some(" hi "), 5).unwrap(), Some("hi".into()));
        assert!(optional("notes", Some("toolong"), 5).is_err());
    }

    #[test]
    fn emails_must_look_like_addresses() {
        assert_eq!(email(Some(" ana@example.com ")).unwrap(), Some("ana@example.com".into()));
        assert_eq!(email(Some("")).unwrap(), None);
        for bad in ["ana", "@example.com", "ana@example", "ana@.com", "ana@example.", "a b@example.com", "a@b@c.com"] {
            assert!(email(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn search_patterns_match_their_text_literally() {
        assert_eq!(contains_pattern(" 50%_off\\ "), r"%50\%\_off\\%");
    }
}
