// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.198269
//+ Revised: 	2026/11/03 04:54:18.280825
// -----------------------------------------------------------------------------

//! Admin passwords, hashed with Argon2id (the argon2 crate's defaults: 19 MiB, 2 passes, which
//! keeps a login on a Raspberry Pi 5 well under a second). Hashing is CPU-heavy, so it runs on the
//! blocking thread pool.

use std::sync::OnceLock;

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};

pub const MIN_LENGTH: usize = 12;
pub const MAX_LENGTH: usize = 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PasswordError {
    #[error("the password must be at least {MIN_LENGTH} characters long")]
    TooShort,
    #[error("the password must be at most {MAX_LENGTH} characters long")]
    TooLong,
    #[error("could not hash the password: {0}")]
    Hashing(String),
}

/// Length is the rule: a long passphrase beats character-class rules (NIST SP 800-63B).
pub fn validate_new(password: &str) -> Result<(), PasswordError> {
    match password.chars().count() {
        length if length < MIN_LENGTH => Err(PasswordError::TooShort),
        length if length > MAX_LENGTH => Err(PasswordError::TooLong),
        _ => Ok(()),
    }
}

pub async fn hash(password: String) -> Result<String, PasswordError> {
    tokio::task::spawn_blocking(move || hash_blocking(&password))
        .await
        .map_err(|error| PasswordError::Hashing(error.to_string()))?
}

fn hash_blocking(password: &str) -> Result<String, PasswordError> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|error| PasswordError::Hashing(error.to_string()))
}

/// Whether `password` matches `stored`. An unreadable stored hash counts as a mismatch.
pub async fn verify(password: String, stored: String) -> bool {
    tokio::task::spawn_blocking(move || verify_blocking(&password, &stored)).await.unwrap_or(false)
}

/// Spends the same effort as a real check, for a username that doesn't exist, so the response
/// time doesn't tell whether an admin account exists.
pub async fn verify_against_nothing(password: String) {
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(dummy) = dummy_hash() {
            verify_blocking(&password, dummy);
        }
    })
    .await;
}

fn verify_blocking(password: &str, stored: &str) -> bool {
    PasswordHash::new(stored)
        .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

fn dummy_hash() -> Option<&'static str> {
    static DUMMY: OnceLock<Option<String>> = OnceLock::new();
    DUMMY.get_or_init(|| hash_blocking("a password nobody has").ok()).as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_is_the_only_rule() {
        assert_eq!(validate_new("short"), Err(PasswordError::TooShort));
        assert_eq!(validate_new(&"x".repeat(11)), Err(PasswordError::TooShort));
        assert_eq!(validate_new(&"x".repeat(12)), Ok(()));
        assert_eq!(validate_new(&"x".repeat(1025)), Err(PasswordError::TooLong));
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        assert_eq!(validate_new("ñññññññññññ"), Err(PasswordError::TooShort), "11 characters, 22 bytes");
    }

    #[tokio::test]
    async fn a_hash_verifies_its_password_only() {
        let stored = hash("correct horse battery".into()).await.unwrap();
        assert!(stored.starts_with("$argon2id$"), "{stored}");
        assert!(verify("correct horse battery".into(), stored.clone()).await);
        assert!(!verify("correct horse battery!".into(), stored).await);
    }

    #[tokio::test]
    async fn the_same_password_hashes_differently_each_time() {
        assert_ne!(
            hash("correct horse battery".into()).await.unwrap(),
            hash("correct horse battery".into()).await.unwrap()
        );
    }

    #[tokio::test]
    async fn an_unreadable_stored_hash_never_matches() {
        assert!(!verify("anything".into(), "not a hash".into()).await);
        verify_against_nothing("anything".into()).await;
    }
}
