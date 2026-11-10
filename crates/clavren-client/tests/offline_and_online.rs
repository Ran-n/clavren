// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.065377
//+ Revised: 	2026/11/10 17:59:19.401907
// -----------------------------------------------------------------------------

//! The client against an in-memory server: activation, offline starts, the daily check, clock
//! rollback, definitive and temporary failures, restarts, and the background checker.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod fake_server;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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

/// Regression: after a check, the system clock became the baseline and tokens were judged by it,
/// so a clock set back two years before one check made the 30-day token last two years offline.
#[test]
fn setting_the_clock_back_before_a_check_gains_nothing() {
    let computer = Computer::with_clock_behind(TimeDelta::zero());
    let client = computer.activated();
    computer.advance(TimeDelta::days(-730));
    assert_eq!(client.status(), Outcome::NeedsCheck);
    assert!(client.check().unwrap().is_licensed(), "the check succeeds on the server's time");

    computer.server.set_mode(Mode::Offline);
    computer.pass(TimeDelta::days(29));
    assert!(computer.start().status().is_licensed());
    computer.pass(TimeDelta::days(1));
    assert_eq!(computer.start().status(), Outcome::NeedsCheck, "30 days by the server's clock, not 2 years");
}

/// Regression: each step back was measured against the latest time seen only, so setting the
/// clock back under an hour, again and again, kept a licence running offline for ever.
#[test]
fn many_small_steps_back_add_up() {
    let computer = Computer::new();
    let client = computer.activated();
    computer.server.set_mode(Mode::Offline);
    for step in [TimeDelta::minutes(59), TimeDelta::minutes(-40), TimeDelta::minutes(30)] {
        computer.advance(step);
        assert!(client.status().is_licensed(), "40 minutes back in all is a normal adjustment");
    }
    computer.advance(TimeDelta::minutes(-30));
    assert_eq!(client.status(), Outcome::NeedsCheck, "70 minutes back in all is not");

    computer.server.set_mode(Mode::Normal);
    assert!(client.check().unwrap().is_licensed(), "a check starts the count again");
    computer.advance(TimeDelta::minutes(-50));
    assert!(client.status().is_licensed());
}

#[test]
fn steps_back_add_up_across_restarts() {
    let computer = Computer::new();
    let client = computer.activated();
    computer.advance(TimeDelta::hours(2));
    assert!(client.status().is_licensed());
    computer.advance(TimeDelta::minutes(-50));
    assert!(client.status().is_licensed());
    client.save_time_seen().unwrap();
    drop(client);

    computer.advance(TimeDelta::minutes(-20));
    assert_eq!(computer.start().status(), Outcome::NeedsCheck, "50 minutes, saved, plus 20 more");
}

/// Regression: the offset to the server's clock came from the answer's unsigned `server_time`, so
/// anything between the two (a proxy) could make a 30-day token last years offline.
#[test]
fn the_servers_time_comes_from_the_signed_token() {
    let computer = Computer::new();
    *computer.server.server_time_shift.lock().unwrap() = TimeDelta::days(-730);
    let client = computer.activated();
    computer.server.set_mode(Mode::Offline);
    computer.advance(TimeDelta::days(29));
    assert!(client.status().is_licensed());
    computer.advance(TimeDelta::days(1));
    assert_eq!(client.status(), Outcome::NeedsCheck, "30 days, whatever server_time said");
}

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
fn a_computer_whose_clock_is_simply_wrong_works_normally() {
    for behind in [TimeDelta::days(400), TimeDelta::days(-400)] {
        let computer = Computer::with_clock_behind(behind);
        let client = computer.activated();
        assert!(client.status().is_licensed(), "{behind}");
        computer.server.set_mode(Mode::Offline);
        computer.pass(TimeDelta::days(29));
        assert!(computer.start().status().is_licensed(), "{behind}: works offline");
        computer.pass(TimeDelta::days(1));
        assert_eq!(computer.start().status(), Outcome::NeedsCheck, "{behind}: for 30 days");
    }
}

/// Regression: a 200 answer without the fields Clavren sends (a proxy's page) wiped the saved
/// licence.
#[test]
fn an_answer_that_isnt_from_clavren_changes_nothing() {
    let computer = Computer::new();
    computer.server.set_mode(Mode::Garbage);
    assert_eq!(computer.start().activate(KEY).unwrap(), Outcome::Unreachable);
    assert_eq!(computer.start().status(), Outcome::NeedsActivation);

    computer.server.set_mode(Mode::Normal);
    let client = computer.activated();
    computer.advance(TimeDelta::days(1));
    computer.server.set_mode(Mode::Garbage);
    assert!(matches!(client.check(), Err(Temporary::Server(_))));
    assert!(computer.start().status().is_licensed(), "the saved token is kept");
    assert!(client.needs_check(), "and the check is still due");
}

/// Regression: a token the client can't verify was stored anyway, which left the background
/// checker asking again at once, in a loop.
#[test]
fn a_token_the_client_cannot_verify_is_never_stored() {
    let computer = Computer::new();
    let client = computer.activated();
    let saved = computer.store.load().unwrap().unwrap();
    computer.advance(TimeDelta::days(1));
    computer.server.set_mode(Mode::UntrustedToken);
    assert!(matches!(client.check(), Err(Temporary::Server(message)) if message.contains("can't be used")));
    assert_eq!(computer.store.load().unwrap().unwrap(), saved);
    assert!(client.status().is_licensed());

    let fresh = Computer::new();
    fresh.server.set_mode(Mode::UntrustedToken);
    assert_eq!(fresh.start().activate(KEY).unwrap(), Outcome::NeedsCheck, "activated, but not runnable yet");
    fresh.server.set_mode(Mode::Normal);
    assert!(fresh.start().check().unwrap().is_licensed());
}

#[test]
fn the_background_checker_backs_off_when_a_check_leaves_nothing_to_run_on() {
    let computer = Computer::new();
    computer.server.set_mode(Mode::UntrustedToken);
    let client = Arc::new(computer.start());
    assert_eq!(client.activate(KEY).unwrap(), Outcome::NeedsCheck);
    let before = computer.server.calls.load(Ordering::SeqCst);
    let checks = Background::spawn(client, |_| {}).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    checks.stop();
    let calls = computer.server.calls.load(Ordering::SeqCst) - before;
    assert!(calls <= 2, "one check (two requests), then backoff; got {calls} requests");
}

/// Regression: every `status()` call saved the state (DPAPI and a file write on the UI thread).
/// Now `status()` never touches the disk; the time seen is saved apart, once it moved on.
#[test]
fn status_never_saves_and_the_time_seen_is_saved_once_it_moved_on() {
    struct Counting(MemoryStore, AtomicUsize);
    impl SecretStore for Counting {
        fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
            self.0.load()
        }
        fn save(&self, plain: &[u8]) -> Result<(), StoreError> {
            self.1.fetch_add(1, Ordering::SeqCst);
            self.0.save(plain)
        }
    }
    let computer = Computer::new();
    let store = Arc::new(Counting(MemoryStore::default(), AtomicUsize::new(0)));
    let client = computer.client_on(Box::new(store.clone()), Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0"));
    assert!(client.activate(KEY).unwrap().is_licensed());
    let after_activation = store.1.load(Ordering::SeqCst);
    for _ in 0..100 {
        computer.advance(TimeDelta::seconds(1));
        assert!(client.status().is_licensed());
    }
    computer.advance(TimeDelta::hours(5));
    client.status();
    assert_eq!(store.1.load(Ordering::SeqCst), after_activation, "status() saves nothing");

    client.save_time_seen().unwrap();
    assert_eq!(store.1.load(Ordering::SeqCst), after_activation + 1);
    computer.advance(TimeDelta::minutes(9));
    client.status();
    client.save_time_seen().unwrap();
    assert_eq!(store.1.load(Ordering::SeqCst), after_activation + 1, "under ten minutes more: nothing to save");

    // the saved time catches a clock set back between runs
    computer.advance(TimeDelta::hours(-3));
    let restarted =
        computer.client_on(Box::new(store.clone()), Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0"));
    assert_eq!(restarted.status(), Outcome::NeedsCheck);
}

#[test]
fn the_background_checker_saves_the_time_seen_when_it_stops() {
    let computer = Computer::new();
    let client = Arc::new(computer.activated());
    computer.advance(TimeDelta::hours(3));
    assert!(client.status().is_licensed());
    Background::spawn(client.clone(), |_| {}).unwrap().stop();
    computer.advance(TimeDelta::hours(-2));
    assert_eq!(computer.start().status(), Outcome::NeedsCheck, "the 3 hours seen were saved");
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

/// Regression: a saved licence that couldn't be read at start (another program had the file open)
/// counted as none, and the next activation saved over it.
#[test]
fn a_saved_licence_that_cannot_be_read_is_never_overwritten() {
    struct Locked(AtomicBool);
    impl SecretStore for Locked {
        fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
            let source = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "in use");
            Err(StoreError::Io { path: "licence.dat".into(), source })
        }
        fn save(&self, _: &[u8]) -> Result<(), StoreError> {
            self.0.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
    let computer = Computer::new();
    let store = Arc::new(Locked(AtomicBool::new(false)));
    let client = computer.client_on(Box::new(store.clone()), Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0"));
    assert_eq!(client.status(), Outcome::NeedsActivation);
    assert!(matches!(client.activate(KEY), Err(StoreError::Unreadable(_))));
    assert!(!store.0.load(Ordering::SeqCst), "nothing was saved over it");
    assert_eq!(computer.server.calls.load(Ordering::SeqCst), 0, "and nothing was sent");
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

#[test]
fn a_token_for_another_activation_is_not_trusted() {
    let computer = Computer::new();
    computer.activated();
    let mut saved: serde_json::Value = serde_json::from_slice(&computer.store.load().unwrap().unwrap()).unwrap();
    saved["activation"] = serde_json::json!(uuid::Uuid::from_u128(99));
    computer.store.save(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(computer.start().status(), Outcome::NeedsCheck);
}

#[test]
fn a_panicking_callback_doesnt_end_the_checks() {
    let computer = Computer::new();
    let client = Arc::new(computer.activated());
    computer.advance(TimeDelta::days(1));
    computer.server.set_mode(Mode::Refuse("licence_revoked", 403));
    let (tell, told) = mpsc::channel();
    let checks = Background::spawn(client.clone(), move |outcome| {
        tell.send(outcome).unwrap();
        panic!("the application's callback failed");
    })
    .unwrap();
    assert_eq!(told.recv_timeout(Duration::from_secs(10)).unwrap(), Outcome::Revoked);
    computer.advance(TimeDelta::hours(3));
    assert_eq!(client.status(), Outcome::Revoked);
    checks.stop(); // the thread is still there to stop, and joins without a panic
}
