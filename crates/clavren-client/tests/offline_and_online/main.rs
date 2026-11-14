// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.065377
//+ Revised: 	2026/11/14 12:05:23.036243
// -----------------------------------------------------------------------------

//! The client against an in-memory server: activation, offline starts, the daily check, clock
//! rollback, definitive and temporary failures, restarts, and the background checker.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod background;
mod clock;
#[path = "../fake_server/mod.rs"]
mod fake_server;
mod refusals;
mod storage;
mod tokens;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use chrono::{DateTime, TimeDelta};
use clavren_client::clock::{Clock, ManualClock};
use clavren_client::store::{MemoryStore, SecretStore, StoreError};
use clavren_client::{Background, Client, Config, Outcome, Temporary};
use fake_server::{FakeServer, Mode, PRODUCT, Scripted};

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

    /// A computer whose clock is `behind` the server's (which keeps its own clock).
    fn with_clock_behind(behind: TimeDelta) -> Self {
        let server_clock = Arc::new(ManualClock::new(DateTime::from_timestamp(1_800_000_000, 0).unwrap()));
        let clock = Arc::new(ManualClock::new(server_clock.now() - behind));
        Self { server: FakeServer::new(server_clock), clock, store: Arc::new(MemoryStore::default()) }
    }

    /// Moves the computer's clock and the server's together: time passing.
    fn pass(&self, by: TimeDelta) {
        self.advance(by);
        if !Arc::ptr_eq(&self.clock, &self.server.clock) {
            self.server.clock.set(self.server.clock.now() + by);
        }
    }

    fn client_on(&self, store: Box<dyn SecretStore>, config: Config) -> Client {
        Client::new(config, store, Box::new(self.server.clone()), Box::new(self.clock.clone()))
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
    assert_eq!(
        outcome,
        Outcome::Licensed { licence_expires_at: None, offline_until: now + TimeDelta::days(30), test: false }
    );

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
fn a_test_licence_is_reported_as_one_online_and_offline() {
    let computer = Computer::new();
    *computer.server.test_licence.lock().unwrap() = true;
    let test = |outcome: Outcome| matches!(outcome, Outcome::Licensed { test: true, .. });
    assert!(test(computer.activated().status()));
    computer.server.set_mode(Mode::Offline);
    assert!(test(computer.start().status()), "the cached token says so too");
    computer.server.set_mode(Mode::Normal);
    *computer.server.test_licence.lock().unwrap() = false;
    computer.advance(TimeDelta::days(1));
    assert!(matches!(computer.start().check().unwrap(), Outcome::Licensed { test: false, .. }));
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
        Outcome::Licensed { licence_expires_at: None, offline_until: checked_at + TimeDelta::days(30), test: false }
    );
    assert!(!client.needs_check());
}

/// Regression: an interval past what the calendar can add panicked instead of meaning "never".
#[test]
fn a_check_interval_too_long_for_the_calendar_means_not_again() {
    let computer = Computer::new();
    computer.activated();
    let mut config = Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0");
    config.check_interval = Duration::MAX;
    let client = computer.client_on(Box::new(computer.store.clone()), config);
    assert_eq!(client.time_until_due(), Duration::MAX);
    assert!(!client.needs_check());
}
