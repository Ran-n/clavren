// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.256555
//+ Revised: 	2026/11/08 14:55:44.297764
// -----------------------------------------------------------------------------

//! One-time recovery codes, for an admin who lost the phone with their authenticator app. Each is
//! 128 random bits in Crockford Base32 (`XXXXX-XXXXX-XXXXX-XXXXX-XXXXX-X`); only its BLAKE3 hash is
//! stored. A slow hash would add nothing: 128 bits can't be guessed.

use clavren_protocol::crockford;

use crate::crypto::random::{RandomError, random_bytes};

pub const COUNT: usize = 10;
const CODE_BYTES: usize = 16;
const SYMBOLS: usize = 26;
const GROUP: usize = 5;

pub fn generate() -> Result<Vec<String>, RandomError> {
    (0..COUNT).map(|_| random_bytes::<CODE_BYTES>().map(|bytes| group(&crockford::encode(&bytes)))).collect()
}

fn group(symbols: &str) -> String {
    let chunks: Vec<&str> =
        symbols.as_bytes().chunks(GROUP).filter_map(|chunk| std::str::from_utf8(chunk).ok()).collect();
    chunks.join("-")
}

/// The canonical form of a typed code, or `None` if it can't be one.
pub fn normalize(typed: &str) -> Option<String> {
    crockford::normalize(typed).filter(|code| code.len() == SYMBOLS)
}

pub fn hash(canonical: &str) -> [u8; 32] {
    *blake3::hash(canonical.as_bytes()).as_bytes()
}

/// The hashes to store for codes just shown, one per code; `None` if any of them doesn't read
/// back as a code, so the admin is never shown a code that wasn't saved.
pub fn hash_all(codes: &[String]) -> Option<Vec<[u8; 32]>> {
    codes.iter().map(|code| normalize(code).map(|canonical| hash(&canonical))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_new_code_is_hashed_or_none_is() {
        let codes = generate().unwrap();
        let hashes = hash_all(&codes).unwrap();
        assert_eq!(hashes.len(), codes.len());
        assert_eq!(hashes[0], hash(&normalize(&codes[0]).unwrap()));
        assert_eq!(hash_all(&[codes[0].clone(), "not a code".into()]), None);
    }

    #[test]
    fn ten_distinct_grouped_codes() {
        let codes = generate().unwrap();
        assert_eq!(codes.len(), COUNT);
        for code in &codes {
            assert_eq!(code.len(), 31, "{code}");
            assert_eq!(code.matches('-').count(), 5, "{code}");
        }
        let mut unique = codes.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), COUNT);
    }

    #[test]
    fn a_code_normalizes_however_it_is_typed() {
        let code = generate().unwrap().remove(0);
        let canonical = normalize(&code).unwrap();
        assert_eq!(normalize(&code.to_lowercase().replace('-', " ")).as_deref(), Some(canonical.as_str()));
        assert_eq!(hash(&canonical), hash(&normalize(&code).unwrap()));
    }

    #[test]
    fn things_that_cannot_be_codes_are_rejected() {
        assert_eq!(normalize("123456"), None, "a TOTP code");
        assert_eq!(normalize(&"A".repeat(27)), None);
        assert_eq!(normalize("ABCDE-FGHJK-MNPQR-STVWX-YZ012-U"), None, "U is not in the alphabet");
    }
}
