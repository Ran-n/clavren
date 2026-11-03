// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.335664
//+ Revised: 	2026/11/03 04:54:12.335664
// -----------------------------------------------------------------------------

//! Licence keys: `BRN-7KPX-4M9D-QW2R-8NFC`, the prefix `BRN` and 16 Crockford Base32 characters
//! (80 random bits) in four groups. Applications and the server read typed keys with the same
//! [`LicenceKey::parse`], so a mistyped key is caught before going online.

use std::fmt;

use crate::crockford;

pub const PREFIX: &str = "BRN";
/// Random bytes per key: 80 bits, 16 Base32 characters.
pub const RANDOM_BYTES: usize = 10;
const BODY_LENGTH: usize = 16;
const GROUP_LENGTH: usize = 4;

/// A well-formed key, held in canonical form: prefix and 16 characters, uppercase, no dashes.
/// `Debug` never shows it, so a key can't end up in a log by accident.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct LicenceKey(String);

impl LicenceKey {
    pub fn from_random(bytes: [u8; RANDOM_BYTES]) -> Self {
        Self(format!("{PREFIX}{}", crockford::encode(&bytes)))
    }

    /// Reads a key as a person typed it: any case, spaces and dashes anywhere, `I`/`L` for `1`
    /// and `O` for `0`. `None` unless the prefix and exactly 16 alphabet characters remain.
    pub fn parse(typed: &str) -> Option<Self> {
        let normalized = crockford::normalize(typed)?;
        let well_formed = normalized.strip_prefix(PREFIX)?.len() == BODY_LENGTH;
        well_formed.then_some(Self(normalized))
    }

    /// `BRN7KPX4M9DQW2R8NFC`: what lookup hashes and encryption work on.
    pub fn canonical(&self) -> &str {
        &self.0
    }

    /// `BRN-7KPX-4M9D-QW2R-8NFC`: how keys are shown to people.
    pub fn formatted(&self) -> String {
        let body = &self.0[PREFIX.len()..];
        let groups: Vec<&str> = (0..BODY_LENGTH).step_by(GROUP_LENGTH).map(|at| &body[at..at + GROUP_LENGTH]).collect();
        format!("{PREFIX}-{}", groups.join("-"))
    }
}

impl fmt::Debug for LicenceKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LicenceKey(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// Published in `docs/api.md` so clients in other languages can check their normalization.
    #[test]
    fn test_vectors() {
        let key = LicenceKey::from_random([0x3A, 0x7D, 0x52, 0x13, 0xB7, 0xE0, 0x04, 0x51, 0x9C, 0xFF]);
        assert_eq!(key.canonical(), "BRN79YN44XQW025377Z");
        assert_eq!(key.formatted(), "BRN-79YN-44XQ-W025-377Z");
        for typed in
            ["BRN-79YN-44XQ-W025-377Z", "brn 79yn 44xq w025 377z", "BRN79YN44XQWO25377Z", " brn-79yn44xq-wO25-377z "]
        {
            assert_eq!(LicenceKey::parse(typed).as_ref(), Some(&key), "{typed}");
        }
    }

    #[test]
    fn malformed_keys_are_refused() {
        for typed in [
            "",
            "BRN",
            "BRN-79YN-44XQ-W025-377",
            "BRN-79YN-44XQ-W025-377ZZ",
            "XYZ-79YN-44XQ-W025-377Z",
            "79YN-44XQ-W025-377Z-BRNA",
            "BRN-79YN-44XQ-W025-377U",
            "BRN-79YN-44XQ-W025-377!",
        ] {
            assert_eq!(LicenceKey::parse(typed), None, "{typed:?} accepted");
        }
    }

    #[test]
    fn debug_output_hides_the_key() {
        let key = LicenceKey::from_random([7; RANDOM_BYTES]);
        assert!(!format!("{key:?}").contains(&key.canonical()[PREFIX.len()..]));
    }

    proptest! {
        #[test]
        fn any_random_key_formats_and_parses_back(bytes in any::<[u8; RANDOM_BYTES]>()) {
            let key = LicenceKey::from_random(bytes);
            prop_assert_eq!(key.canonical().len(), PREFIX.len() + BODY_LENGTH);
            prop_assert_eq!(LicenceKey::parse(&key.formatted()), Some(key.clone()));
            prop_assert_eq!(LicenceKey::parse(&key.formatted().to_lowercase().replace('-', " ")), Some(key));
        }

        #[test]
        fn a_character_outside_the_alphabet_is_refused(bytes in any::<[u8; RANDOM_BYTES]>(), at in 4usize..23, bad in "[!-/:-@U\\[-`u{-~]") {
            let mut typed: Vec<char> = LicenceKey::from_random(bytes).formatted().chars().collect();
            typed[at] = bad.chars().next().unwrap();
            let typed: String = typed.into_iter().collect();
            prop_assume!(!matches!(typed.chars().nth(at), Some('-')));
            prop_assert_eq!(LicenceKey::parse(&typed), None);
        }
    }
}
