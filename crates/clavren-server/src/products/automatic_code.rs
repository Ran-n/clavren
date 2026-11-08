// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/07 19:36:57.349794
//+ Revised: 	2026/11/07 19:36:57.349794
// -----------------------------------------------------------------------------

//! Codes made from product names: accents dropped, lowercase, every other run of characters a
//! dash, and `-2`, `-3`, … when the code is taken. A typed code is never changed this way.

use sqlx::PgConnection;
use unicode_normalization::UnicodeNormalization;

use super::{MAX_CODE, code_taken_error, repository};
use crate::http::error::ApiError;

/// What a name with nothing usable in it (only symbols, or a script without Latin letters) becomes.
const FALLBACK: &str = "product";

/// How many suffixes are tried before giving up; never reached in practice, it bounds the loop.
const MAX_SUFFIX: u32 = 1000;

/// The code `name` reads as: decomposed (NFKD) so accents fall off, ASCII letters and digits kept
/// in lowercase, every other run of characters one `-`, at most [`MAX_CODE`] characters.
pub fn from_name(name: &str) -> String {
    let mut code = String::with_capacity(name.len());
    for character in name.nfkd().filter(char::is_ascii) {
        if character.is_ascii_alphanumeric() {
            code.push(character.to_ascii_lowercase());
        } else if !code.is_empty() && !code.ends_with('-') {
            code.push('-');
        }
    }
    let code = fit(&code, MAX_CODE);
    if code.is_empty() { FALLBACK.to_string() } else { code.to_string() }
}

/// `base` for the first try, then `<base>-<n>`, the base cut so the whole fits in [`MAX_CODE`].
pub fn with_suffix(base: &str, n: u32) -> String {
    if n == 1 {
        return base.to_string();
    }
    let suffix = format!("-{n}");
    format!("{}{suffix}", fit(base, MAX_CODE - suffix.len()))
}

/// The first code made from `name` that no product has yet.
pub async fn choose(connection: &mut PgConnection, name: &str) -> Result<String, ApiError> {
    let base = from_name(name);
    for n in 1..=MAX_SUFFIX {
        let code = with_suffix(&base, n);
        if !repository::code_exists(&mut *connection, &code).await? {
            return Ok(code);
        }
    }
    Err(code_taken_error())
}

/// `code` cut to `max` characters (it is ASCII), without a dash left at either end.
fn fit(code: &str, max: usize) -> &str {
    code[..code.len().min(max)].trim_matches('-')
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn names_become_codes() {
        for (name, code) in [
            (" Avalio Lite ", "avalio-lite"),
            ("Avalio  Lite (2026)", "avalio-lite-2026"),
            ("C++ / Rust", "c-rust"),
            ("Ñandú Pro 2", "nandu-pro-2"),
            ("Straße", "strae"),
            ("Café-Bar", "cafe-bar"),
        ] {
            assert_eq!(from_name(name), code, "{name:?}");
        }
        let long = from_name(&format!("{} {}", "a".repeat(62), "b".repeat(40)));
        assert_eq!(long, "a".repeat(62), "cut to 63, without the dash the cut left");
    }

    #[test]
    fn names_with_nothing_usable_become_product() {
        for name in ["日本語", "!!!", " ", ""] {
            assert_eq!(from_name(name), "product", "{name:?}");
        }
    }

    #[test]
    fn suffixes_fit_in_63_characters() {
        assert_eq!(with_suffix(&"a".repeat(63), 2), format!("{}-2", "a".repeat(61)));
        assert_eq!(with_suffix("avalio", 1), "avalio");
        assert_eq!(with_suffix("avalio", 1000), "avalio-1000");
        let base = format!("{}-{}", "a".repeat(60), "bc");
        assert_eq!(with_suffix(&base, 2), format!("{}-2", "a".repeat(60)), "no `--` where the cut ends in a dash");
    }

    proptest! {
        /// A made code always passes the same check as a typed one, so creating never fails on it.
        #[test]
        fn any_name_gives_a_valid_code(name in any::<String>(), n in 1u32..=MAX_SUFFIX) {
            let code = with_suffix(&from_name(&name), n);
            prop_assert!(super::super::code(&code).is_ok(), "{code:?}");
            prop_assert_eq!(super::super::code(&code).unwrap(), code);
        }
    }
}
