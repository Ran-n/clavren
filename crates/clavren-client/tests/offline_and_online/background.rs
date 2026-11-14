// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:22.846989
//+ Revised: 	2026/11/14 17:15:42.342071
// -----------------------------------------------------------------------------

//! The background checker.

use super::*;

#[test]
fn the_background_checker_backs_off_when_a_check_leaves_nothing_to_run_on() {
    let computer = Computer::new();
    computer.server.set_mode(Mode::UntrustedToken);
    let client = Arc::new(computer.start());
    assert_eq!(client.activate(KEY).unwrap(), Outcome::NeedsCheck);
    let before = computer.server.calls.load(Ordering::SeqCst);
    let checks = Background::spawn(client, |_| {}).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while computer.server.calls.load(Ordering::SeqCst) - before < 2 {
        assert!(std::time::Instant::now() < deadline, "the due check never ran");
        std::thread::sleep(Duration::from_millis(10));
    }
    // long enough for a checker without backoff to ask many times over
    std::thread::sleep(Duration::from_millis(300));
    checks.stop();
    let calls = computer.server.calls.load(Ordering::SeqCst) - before;
    assert_eq!(calls, 2, "one check (two requests), then backoff");
}

#[test]
fn the_background_checker_saves_the_time_seen_when_it_stops() {
    let computer = Computer::new();
    let client = Arc::new(computer.activated());
    let checks = Background::spawn(client.clone(), |_| {}).unwrap();
    // what is seen after the checker's first round waits for the stop to be saved. (When the
    // thread is slow to start, its first round saves it instead: the same state either way, so
    // only what is saved once it has stopped is compared.)
    computer.advance(TimeDelta::hours(3));
    assert!(client.status().is_licensed());
    checks.stop();
    let saved: serde_json::Value = serde_json::from_slice(&computer.store.load().unwrap().unwrap()).unwrap();
    let seen: DateTime<chrono::Utc> = serde_json::from_value(saved["latest_seen"].clone()).unwrap();
    assert_eq!(seen, computer.clock.now(), "the 3 hours seen are saved once it has stopped");
    computer.advance(TimeDelta::hours(-2));
    assert_eq!(computer.start().status(), Outcome::NeedsCheck, "so a clock set back between runs is noticed");
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
fn a_panicking_callback_doesnt_end_the_checks() {
    let computer = Computer::new();
    let client = Arc::new(computer.activated());
    computer.advance(TimeDelta::days(1));
    // a product withdrawn keeps the activation, so checks go on (a revoked one is never asked again)
    computer.server.set_mode(Mode::Refuse("product_unavailable", 403));
    let (tell, told) = mpsc::channel();
    let checks = Background::spawn(client.clone(), move |outcome| {
        tell.send(outcome).unwrap();
        panic!("the application's callback failed");
    })
    .unwrap();
    assert_eq!(told.recv_timeout(Duration::from_secs(10)).unwrap(), Outcome::ProductUnavailable);
    // the checker is still there: once the product is back, its next check finds out
    computer.server.set_mode(Mode::Normal);
    let calls = computer.server.calls.load(Ordering::SeqCst);
    computer.advance(TimeDelta::days(1));
    drop(checks); // stopping wakes the loop; a new checker stands in for the 10-minute wake-up
    let checks = Background::spawn(client.clone(), |_| {}).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !client.status().is_licensed() {
        assert!(std::time::Instant::now() < deadline, "no check ran after the callback panicked");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(computer.server.calls.load(Ordering::SeqCst) > calls);
    checks.stop();
}

/// The checker has nowhere to report a save that failed: it logs it and carries on.
#[test]
fn a_time_seen_that_cannot_be_saved_never_stops_the_checker() {
    struct ReadOnly;
    impl SecretStore for ReadOnly {
        fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
            Ok(None)
        }
        fn save(&self, _: &[u8]) -> Result<(), StoreError> {
            Err(StoreError::Corrupt)
        }
    }
    let computer = Computer::new();
    let config = Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0");
    let client = Arc::new(computer.client_on(Box::new(ReadOnly), config));
    assert_eq!(client.status(), Outcome::NeedsActivation); // a time seen, so there is something to save
    assert!(client.save_time_seen().is_err());
    let checks = Background::spawn(client.clone(), |_| {}).unwrap();
    checks.stop();
    assert_eq!(client.status(), Outcome::NeedsActivation);
}
