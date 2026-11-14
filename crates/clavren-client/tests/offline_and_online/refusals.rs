// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:22.643161
//+ Revised: 	2026/11/14 17:01:17.192664
// -----------------------------------------------------------------------------

//! What the server refuses, and failures that change nothing.

use super::*;

/// Regression: a refusal was kept in memory even when it couldn't be saved, so the application
/// locked while the disk still held the licence.
#[test]
fn a_refusal_that_cannot_be_saved_changes_nothing() {
    struct Switchable(MemoryStore, AtomicBool);
    impl SecretStore for Switchable {
        fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
            self.0.load()
        }
        fn save(&self, plain: &[u8]) -> Result<(), StoreError> {
            if self.1.load(Ordering::SeqCst) { Err(StoreError::Corrupt) } else { self.0.save(plain) }
        }
    }
    let computer = Computer::new();
    let store = Arc::new(Switchable(MemoryStore::default(), AtomicBool::new(false)));
    let client = computer.client_on(Box::new(store.clone()), Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0"));
    assert!(client.activate(KEY).unwrap().is_licensed());
    store.1.store(true, Ordering::SeqCst);
    computer.server.set_mode(Mode::Refuse("licence_revoked", 403));
    computer.advance(TimeDelta::days(1));
    assert!(matches!(client.check(), Err(Temporary::Storage(_))));
    assert!(client.status().is_licensed(), "nothing changed in memory either");
    assert!(client.needs_check(), "the check is still due");
}

#[test]
fn a_revoked_or_reset_licence_locks_for_good() {
    let computer = Computer::new();
    let client = computer.activated();
    computer.server.set_mode(Mode::Refuse("activation_revoked", 403));
    computer.advance(TimeDelta::days(1));
    assert_eq!(client.check().unwrap(), Outcome::Revoked);
    assert_eq!(client.status(), Outcome::Revoked, "even though the token hadn't run out");
    assert!(!client.needs_check(), "nothing left to check");
    assert_eq!(computer.start().status(), Outcome::Revoked, "it survives a restart");

    computer.server.set_mode(Mode::Normal);
    assert!(computer.start().activate(KEY).unwrap().is_licensed(), "a key (after a reset) activates again");
}

#[test]
fn an_expired_licence_says_when_and_is_checked_again_later() {
    let computer = Computer::new();
    let ends = computer.clock.now() + TimeDelta::days(2);
    *computer.server.licence_expires_at.lock().unwrap() = Some(ends);
    let client = computer.activated();
    assert_eq!(client.status(), Outcome::Licensed { licence_expires_at: Some(ends), offline_until: ends, test: false });

    computer.advance(TimeDelta::days(1));
    computer.server.set_mode(Mode::Refuse("licence_expired", 403));
    assert_eq!(client.check().unwrap(), Outcome::Expired { at: Some(ends) });
    assert!(!client.needs_check(), "not again at once");
    computer.advance(TimeDelta::days(1));
    assert!(client.needs_check(), "the administrator may extend it");
    computer.server.set_mode(Mode::Normal);
    *computer.server.licence_expires_at.lock().unwrap() = None;
    assert!(client.check().unwrap().is_licensed(), "extended");
}

#[test]
fn a_withdrawn_product_locks() {
    let computer = Computer::new();
    let client = computer.activated();
    computer.server.set_mode(Mode::Refuse("product_unavailable", 403));
    assert_eq!(client.check().unwrap(), Outcome::ProductUnavailable);
}

#[test]
fn temporary_failures_change_nothing() {
    let computer = Computer::new();
    let client = computer.activated();
    for (mode, expected) in [
        (Mode::Refuse("rate_limited", 429), "RateLimited"),
        (Mode::Refuse("internal_error", 500), "Server"),
        (Mode::Refuse("challenge_invalid", 401), "Server"),
        (Mode::Offline, "Unreachable"),
    ] {
        computer.server.set_mode(mode);
        let problem = client.check().unwrap_err();
        assert!(format!("{problem:?}").starts_with(expected), "{problem:?}");
        assert!(client.status().is_licensed());
    }
    computer.server.set_mode(Mode::Refuse("rate_limited", 429));
    assert!(matches!(client.check(), Err(Temporary::RateLimited(wait)) if wait == Duration::from_secs(120)));
}

#[test]
fn activation_refusals_become_outcomes() {
    for (code, status, expected) in [
        ("invalid_key", 404, Outcome::InvalidKey),
        ("already_activated", 409, Outcome::AlreadyActivatedElsewhere),
        ("licence_revoked", 403, Outcome::Revoked),
        ("licence_expired", 403, Outcome::Expired { at: None }),
        ("product_unavailable", 403, Outcome::ProductUnavailable),
        ("rate_limited", 429, Outcome::RateLimited { retry_after: Duration::from_secs(120) }),
        ("internal_error", 500, Outcome::Unreachable),
    ] {
        let computer = Computer::new();
        computer.server.set_mode(Mode::Refuse(code, status));
        assert_eq!(computer.start().activate(KEY).unwrap(), expected, "{code}");
        assert_eq!(computer.start().status(), Outcome::NeedsActivation, "{code}: nothing is kept");
    }
    let computer = Computer::new();
    computer.server.set_mode(Mode::Offline);
    assert_eq!(computer.start().activate(KEY).unwrap(), Outcome::Unreachable);
}

#[test]
fn a_passed_check_without_a_token_changes_nothing() {
    let computer = Computer::new();
    let client = computer.activated();
    computer.advance(TimeDelta::days(1));
    computer
        .server
        .script("/checks", Scripted::Answer(200, serde_json::json!({ "server_time": computer.clock.now() })));
    assert!(matches!(client.check(), Err(Temporary::Server(_))));
    assert!(client.status().is_licensed(), "the token it had still holds");
    assert!(client.needs_check(), "and the check is still due");
}
