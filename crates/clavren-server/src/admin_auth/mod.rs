// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.139245
//+ Revised: 	2026/11/03 04:54:18.201542
// -----------------------------------------------------------------------------

//! Administrator access: accounts (password + authenticator app + recovery codes), login,
//! sessions, CSRF protection and account settings. Accounts are created with the
//! `clavren-admin` CLI; there is no registration.

pub mod account;
pub mod cookie;
pub mod factors;
pub mod guard;
pub mod login;
pub mod passwords;
pub mod recovery_codes;
pub mod repository;
mod routes;
pub mod sessions;
pub mod totp;

pub use routes::routes;

/// The rule the database also enforces: lowercase letters, digits, `.`, `_` and `-`, starting with
/// a letter or digit, up to 63 characters.
pub fn is_valid_username(username: &str) -> bool {
    let mut characters = username.chars();
    let starts_well = characters.next().is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit());
    starts_well
        && username.len() <= 63
        && characters
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || "._-".contains(character))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_follow_the_database_rule() {
        for valid in ["ran", "ran.hash", "a", "admin_2", "x-y", &"a".repeat(63)] {
            assert!(is_valid_username(valid), "{valid} rejected");
        }
        for invalid in ["", "Ran", "_ran", "-ran", "ran hash", "ñandú", &"a".repeat(64)] {
            assert!(!is_valid_username(invalid), "{invalid} accepted");
        }
    }
}
