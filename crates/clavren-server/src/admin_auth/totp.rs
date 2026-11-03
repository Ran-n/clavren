// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.493531
//+ Revised: 	2026/11/03 04:54:18.647655
// -----------------------------------------------------------------------------

//! Authenticator-app codes (TOTP, RFC 6238): SHA-1, 6 digits, 30-second steps, one step of
//! tolerance either side, which is what every authenticator app supports. The secret is stored
//! encrypted, bound to its admin's ID.

use chrono::{DateTime, Utc};
use totp_rs::{Algorithm, Builder, Secret, Totp};
use uuid::Uuid;

use crate::crypto::aead::{self, AeadError, Sealed};
use crate::crypto::key_ring::KeyRing;
use crate::crypto::random::{RandomError, random_bytes};

pub const SECRET_LENGTH: usize = 20;
const ISSUER: &str = "Clavren";
const DIGITS: u8 = 6;
const STEP_SECONDS: u64 = 30;
const SKEW_STEPS: u16 = 1;

#[derive(Debug, thiserror::Error)]
pub enum TotpError {
    #[error("the authenticator secret is not usable: {0}")]
    Secret(String),
    #[error(transparent)]
    Random(#[from] RandomError),
    #[error(transparent)]
    Stored(#[from] AeadError),
}

pub fn new_secret() -> Result<[u8; SECRET_LENGTH], TotpError> {
    Ok(random_bytes::<SECRET_LENGTH>()?)
}

fn totp(secret: &[u8], username: &str) -> Result<Totp, TotpError> {
    Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(DIGITS)
        .with_step_duration(STEP_SECONDS)
        .with_skew(SKEW_STEPS)
        .with_secret(secret.to_vec())
        .with_account_name(username)
        .with_issuer(Some(ISSUER))
        .build()
        .map_err(|error| TotpError::Secret(error.to_string()))
}

/// The `otpauth://` address an authenticator app reads from the QR code.
pub fn setup_uri(secret: &[u8], username: &str) -> Result<String, TotpError> {
    totp(secret, username)?.to_url().map_err(|error| TotpError::Secret(error.to_string()))
}

/// The secret in Base32, for typing into an app that can't scan the QR code.
pub fn secret_text(secret: &[u8]) -> String {
    Secret::new(secret.into()).to_base32()
}

pub fn secret_from_text(text: &str) -> Option<Vec<u8>> {
    let secret = Secret::try_from_base32(text.trim().replace(' ', "").to_ascii_uppercase()).ok()?;
    (secret.as_bytes().len() == SECRET_LENGTH).then(|| secret.as_bytes().to_vec())
}

/// The time step `code` belongs to, if it is valid at `now`. The caller records it, so the same
/// code (or an older one) can't be used twice.
pub fn matching_step(secret: &[u8], username: &str, code: &str, now: DateTime<Utc>) -> Option<u64> {
    let seconds = u64::try_from(now.timestamp()).ok()?;
    let is_code = code.len() == usize::from(DIGITS) && code.bytes().all(|byte| byte.is_ascii_digit());
    if !is_code {
        return None;
    }
    totp(secret, username).ok()?.check(code, seconds)
}

pub fn seal(keys: &KeyRing, admin_id: Uuid, secret: &[u8]) -> Result<Sealed, TotpError> {
    Ok(aead::seal(keys.totp_encryption(), secret, admin_id.as_bytes())?)
}

pub fn open(keys: &KeyRing, admin_id: Uuid, ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, TotpError> {
    Ok(aead::open(keys.totp_encryption(), ciphertext, nonce, admin_id.as_bytes())?)
}

/// The code an authenticator app shows at `at` (tests and the CLI's own check).
pub fn code_at(secret: &[u8], username: &str, at: DateTime<Utc>) -> Result<String, TotpError> {
    let seconds = u64::try_from(at.timestamp()).map_err(|_| TotpError::Secret("time before 1970".into()))?;
    Ok(totp(secret, username)?.generate(seconds).to_string())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::crypto::master_key::MasterKey;

    /// RFC 6238 appendix B's SHA-1 secret and its published 8-digit code at 59 s is 94287082;
    /// the 6-digit code is its last six digits.
    #[test]
    fn rfc_6238_test_vector() {
        let secret = b"12345678901234567890";
        let at = DateTime::from_timestamp(59, 0).unwrap();
        assert_eq!(code_at(secret, "rfc", at).unwrap(), "287082");
        assert_eq!(matching_step(secret, "rfc", "287082", at), Some(1));
    }

    #[test]
    fn a_code_is_accepted_one_step_either_side_only() {
        let secret = new_secret().unwrap();
        let at = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let code = code_at(&secret, "ran", at).unwrap();
        assert!(matching_step(&secret, "ran", &code, at + Duration::from_secs(30)).is_some());
        assert!(matching_step(&secret, "ran", &code, at - Duration::from_secs(30)).is_some());
        assert_eq!(matching_step(&secret, "ran", &code, at + Duration::from_secs(90)), None);
    }

    #[test]
    fn malformed_codes_are_rejected() {
        let secret = new_secret().unwrap();
        let at = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        for code in ["", "12345", "1234567", "12a456", " 123456"] {
            assert_eq!(matching_step(&secret, "ran", code, at), None, "{code:?} accepted");
        }
    }

    #[test]
    fn the_setup_uri_names_issuer_and_account() {
        let uri = setup_uri(&new_secret().unwrap(), "ran").unwrap();
        assert!(uri.starts_with("otpauth://totp/Clavren:ran?"), "{uri}");
        assert!(uri.contains("issuer=Clavren"), "{uri}");
    }

    #[test]
    fn the_secret_text_round_trips() {
        let secret = new_secret().unwrap();
        let text = secret_text(&secret);
        assert_eq!(secret_from_text(&text.to_lowercase()), Some(secret.to_vec()));
        assert_eq!(secret_from_text("not base32!"), None);
        assert_eq!(secret_from_text("JBSWY3DP"), None, "too short");
    }

    #[test]
    fn a_sealed_secret_opens_only_for_its_admin() {
        let keys = KeyRing::new(MasterKey::from_bytes([3; 32]));
        let (admin, other) = (Uuid::now_v7(), Uuid::now_v7());
        let secret = new_secret().unwrap();
        let sealed = seal(&keys, admin, &secret).unwrap();
        assert_eq!(open(&keys, admin, &sealed.ciphertext, &sealed.nonce).unwrap(), secret);
        assert!(open(&keys, other, &sealed.ciphertext, &sealed.nonce).is_err());
    }
}
