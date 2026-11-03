// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.065377
//+ Revised: 	2026/11/03 04:54:12.065377
// -----------------------------------------------------------------------------

//! The client against an in-memory server: activation, offline starts, the daily check, clock
//! rollback, definitive and temporary failures, restarts, and the background checker.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod fake_server;

use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use chrono::{DateTime, TimeDelta};
use clavren_client::clock::{Clock, ManualClock};
use clavren_client::store::{MemoryStore, SecretStore, StoreError};
use clavren_client::{Background, Client, Config, Outcome, Temporary};
use fake_server::{FakeServer, Mode, PRODUCT};

const KEY: &str = "BRN-79YN-44XQ-W025-377Z";

struct Computer {
    clock: Arc<ManualClock>,
    server: Arc<FakeServer>,
    store: Arc<MemoryStore>,
}

impl Computer {
    fn new() -> Self {
        let clock = Arc::new(ManualClock::new(DateTime::from_timestamp(1_800_000_000, 0).unwrap()));
        Self { server: FakeServer::new(clock.clone()), clock, store: Arc::new(MemoryStore::default()) }
    }

    /// The application starting: a new client on the same saved state.
    fn start(&self) -> Client {
        let config = Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0");
        Client::new(config, Box::new(self.store.clone()), Box::new(self.server.clone()), Box::new(self.clock.clone()))
    }

    fn advance(&self, by: TimeDelta) {
        self.clock.set(self.clock.now() + by);
    }

    fn activated(&self) -> Client {
        let client = self.start();
        assert!(client.activate(KEY).unwrap().is_licensed());
        client
    }
}

#[test]
fn a_new_computer_needs_activation_and_nothing_to_check() {
    let computer = Computer::new();
    let client = computer.start();
    assert_eq!(client.status(), Outcome::NeedsActivation);
    assert!(!client.needs_check());
    assert_eq!(client.check().unwrap(), Outcome::NeedsActivation);
    assert_eq!(computer.server.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn a_mistyped_key_is_caught_without_going_online() {
    let computer = Computer::new();
    assert_eq!(computer.start().activate("BRN-79YN-44XQ").unwrap(), Outcome::InvalidKey);
    assert_eq!(computer.server.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn an_activated_licence_works_offline_for_thirty_days() {
    let computer = Computer::new();
    let now = computer.clock.now();
    let outcome = computer.activated().status();
    assert_eq!(outcome, Outcome::Licensed { licence_expires_at: None, offline_until: now + TimeDelta::days(30) });

    computer.server.set_mode(Mode::Offline);
    computer.advance(TimeDelta::days(29));
    let restarted = computer.start();
    assert!(restarted.status().is_licensed(), "a restart starts at once, offline");
    assert!(restarted.needs_check());
    assert!(matches!(restarted.check(), Err(Temporary::Unreachable(_))));
    assert!(restarted.status().is_licensed(), "a failed check changes nothing");

    computer.advance(TimeDelta::days(1));
    assert_eq!(computer.start().status(), Outcome::NeedsCheck);
}

#[test]
fn a_check_is_due_once_a_day_and_renews_the_token() {
    let computer = Computer::new();
    let client = computer.activated();
    assert!(!client.needs_check());
    computer.advance(TimeDelta::hours(23));
    assert!(!client.needs_check());
    assert_eq!(client.time_until_due(), Duration::from_secs(3600));
    computer.advance(TimeDelta::hours(1));
    assert!(client.needs_check());

    let checked_at = computer.clock.now();
    assert_eq!(
        client.check().unwrap(),
        Outcome::Licensed { licence_expires_at: None, offline_until: checked_at + TimeDelta::days(30) }
    );
    assert!(!client.needs_check());
}

#[test]
fn setting_the_clock_back_needs_an_online_check() {
    let computer = Computer::new();
    let client = computer.activated();
    computer.advance(TimeDelta::days(3));
    assert!(client.status().is_licensed());

    computer.advance(TimeDelta::minutes(-59));
    assert!(client.status().is_licensed(), "within an hour: a normal clock adjustment");
    computer.advance(TimeDelta::hours(-2));
    assert_eq!(client.status(), Outcome::NeedsCheck);
    assert!(client.needs_check());
    assert!(client.check().unwrap().is_licensed(), "the server's time puts things right");
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
    assert_eq!(client.status(), Outcome::Licensed { licence_expires_at: Some(ends), offline_until: ends });

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
fn the_device_key_is_kept_so_retrying_is_the_same_computer() {
    let computer = Computer::new();
    computer.server.set_mode(Mode::Offline);
    computer.start().activate(KEY).unwrap();
    let saved = computer.store.load().unwrap().unwrap();
    computer.server.set_mode(Mode::Normal);
    assert!(computer.start().activate(KEY).unwrap().is_licensed());
    let key_of = |bytes: &[u8]| serde_json::from_slice::<serde_json::Value>(bytes).unwrap()["device_key"].clone();
    assert_eq!(key_of(&saved), key_of(&computer.store.load().unwrap().unwrap()));
}

#[test]
fn a_token_from_another_device_is_not_trusted() {
    let computer = Computer::new();
    computer.activated();
    let mut saved: serde_json::Value = serde_json::from_slice(&computer.store.load().unwrap().unwrap()).unwrap();
    saved["device_key"] = serde_json::json!(clavren_protocol::encoding::encode(&[9; 32]));
    computer.store.save(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(computer.start().status(), Outcome::NeedsCheck);
}

#[test]
fn unreadable_state_starts_over() {
    struct Broken;
    impl SecretStore for Broken {
        fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
            Err(StoreError::Corrupt)
        }
        fn save(&self, _: &[u8]) -> Result<(), StoreError> {
            Err(StoreError::Corrupt)
        }
    }
    let computer = Computer::new();
    let config = Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0");
    let client =
        Client::new(config, Box::new(Broken), Box::new(computer.server.clone()), Box::new(computer.clock.clone()));
    assert_eq!(client.status(), Outcome::NeedsActivation);
    assert!(client.activate(KEY).is_err(), "a device key that can't be saved stops the activation");
    assert_eq!(computer.server.calls.load(Ordering::SeqCst), 0);

    computer.store.save(b"not json").unwrap();
    assert_eq!(computer.start().status(), Outcome::NeedsActivation);
}

#[test]
fn the_background_checker_reports_a_change_and_stops_promptly() {
    let computer = Computer::new();
    let client = Arc::new(computer.activated());
    computer.advance(TimeDelta::days(1));
    computer.server.set_mode(Mode::Refuse("licence_revoked", 403));
    let (tell, told) = mpsc::channel();
    let checks = Background::spawn(client.clone(), move |outcome| tell.send(outcome).unwrap()).unwrap();

    assert_eq!(told.recv_timeout(Duration::from_secs(10)).unwrap(), Outcome::Revoked);
    let started = std::time::Instant::now();
    checks.stop();
    assert!(started.elapsed() < Duration::from_secs(5), "stopping waits for nothing");
    assert_eq!(client.status(), Outcome::Revoked);
}

#[test]
fn the_background_checker_says_nothing_while_nothing_changes() {
    let computer = Computer::new();
    let client = Arc::new(computer.activated());
    computer.advance(TimeDelta::days(1));
    let (tell, told) = mpsc::channel();
    let checks = Background::spawn(client.clone(), move |outcome| tell.send(outcome).unwrap()).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while client.needs_check() {
        assert!(std::time::Instant::now() < deadline, "the due check never ran");
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(checks);
    assert!(told.try_recv().is_err(), "still licensed: nothing to report");
}
