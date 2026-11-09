// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.198269
//+ Revised: 	2026/11/08 14:55:44.230643
// -----------------------------------------------------------------------------

//! Admin passwords, hashed with Argon2id (the argon2 crate's defaults: 19 MiB, 2 passes, which
//! keeps a login on a Raspberry Pi 5 well under a second). Hashing is CPU-heavy, so it runs on the
//! blocking thread pool, at most [`PARALLEL_HASHES`] at a time: a burst of sign-in attempts then
//! waits in line instead of taking 19 MiB each from a Pi with 1 GB.

use std::sync::OnceLock;

use tokio::sync::Semaphore;

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};

pub const MIN_LENGTH: usize = 12;
pub const MAX_LENGTH: usize = 1024;
/// Argon2 runs at most this many at once (one admin signs in at a time; a few to spare).
pub const PARALLEL_HASHES: usize = 2;

static HASHING: Semaphore = Semaphore::const_new(PARALLEL_HASHES);

/// Runs `work` on the blocking pool once a hashing slot is free.
async fn hashing<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    // the semaphore is never closed, so acquiring only waits
    let _permit = HASHING.acquire().await.map_err(|error| error.to_string())?;
    tokio::task::spawn_blocking(work).await.map_err(|error| error.to_string())
}

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
    hashing(move || hash_blocking(&password)).await.map_err(PasswordError::Hashing)?
}

fn hash_blocking(password: &str) -> Result<String, PasswordError> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|error| PasswordError::Hashing(error.to_string()))
}

/// Whether `password` matches `stored`. An unreadable stored hash counts as a mismatch, and is
/// logged: it means the database holds something Clavren didn't write.
pub async fn verify(password: String, stored: String) -> bool {
    hashing(move || verify_blocking(&password, &stored)).await.unwrap_or_else(|error| {
        tracing::error!("the password check stopped: {error}");
        false
    })
}

/// Spends the same effort as a real check, for a username that doesn't exist, so the response
/// time doesn't tell whether an admin account exists.
pub async fn verify_against_nothing(password: String) {
    let checked = hashing(move || match dummy_hash() {
        Some(dummy) => {
            verify_blocking(&password, dummy);
            Ok(())
        }
        None => Err("there is no stand-in hash".to_string()),
    })
    .await;
    if let Err(error) | Ok(Err(error)) = checked {
        // the answer is the same; only its timing may now tell that the account doesn't exist
        tracing::error!("the stand-in password check didn't run: {error}");
    }
}

fn verify_blocking(password: &str, stored: &str) -> bool {
    match PasswordHash::new(stored) {
        Ok(parsed) => Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok(),
        Err(error) => {
            tracing::error!("a stored password hash can't be read: {error}");
            false
        }
    }
}

fn dummy_hash() -> Option<&'static str> {
    static DUMMY: OnceLock<Option<String>> = OnceLock::new();
    DUMMY
        .get_or_init(|| {
            hash_blocking("a password nobody has")
                .inspect_err(|error| tracing::error!("couldn't make the stand-in password hash: {error}"))
                .ok()
        })
        .as_deref()
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
