// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:48.386270
//+ Revised: 	2026/11/12 23:57:43.489075
// -----------------------------------------------------------------------------

//! Applying for a licence from the application: sending one, waiting (across restarts), the key
//! activated by itself once approved, a rejection with its reason, every refusal of the sending,
//! and the background checker asking until it is decided.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod fake_server;

use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use chrono::{DateTime, TimeDelta};
use clavren_client::clock::{Clock as _, ManualClock};
use clavren_client::store::MemoryStore;
use clavren_client::{Applicant, Applied, Background, Client, Config, Outcome, Temporary};
use fake_server::{APPROVED_KEY, Decision, FakeServer, Mode, PRODUCT};

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

    fn start(&self) -> Client {
        let config = Config::new(PRODUCT, FakeServer::trusted_keys(), "2.0.0");
        Client::new(config, Box::new(self.store.clone()), Box::new(self.server.clone()), Box::new(self.clock.clone()))
    }

    fn advance(&self, by: TimeDelta) {
        self.clock.set(self.clock.now() + by);
    }

    fn decide(&self, decision: Decision) {
        *self.server.decision.lock().unwrap() = decision;
    }

    /// A client that sent an application.
    fn applied(&self) -> Client {
        let client = self.start();
        assert_eq!(client.apply(&ana()).unwrap(), Applied::Sent);
        client
    }
}

fn ana() -> Applicant {
    Applicant { name: "Ana López".into(), email: "ana@example.com".into(), message: Some("For my class".into()) }
}

#[test]
fn a_sent_application_waits_across_restarts() {
    let computer = Computer::new();
    let client = computer.applied();
    let sent = computer.server.applications.lock().unwrap()[0].clone();
    assert_eq!(
        (&sent["name"], &sent["email"], &sent["message"]),
        (&"Ana López".into(), &"ana@example.com".into(), &"For my class".into())
    );
    assert_eq!(client.status(), Outcome::ApplicationPending);
    assert!(client.needs_check(), "asked about at once");
    assert_eq!(computer.start().status(), Outcome::ApplicationPending, "the secret was saved");
}

#[test]
fn a_waiting_application_is_asked_about_every_15_minutes() {
    let computer = Computer::new();
    let client = computer.applied();
    assert_eq!(client.check().unwrap(), Outcome::ApplicationPending);
    assert_eq!(client.time_until_due(), Duration::from_secs(15 * 60));
    computer.advance(TimeDelta::minutes(14));
    assert!(!client.needs_check());
    computer.advance(TimeDelta::minutes(1));
    assert!(client.needs_check());
    assert_eq!(computer.start().time_until_due(), Duration::ZERO, "the last question was saved too");
}

#[test]
fn an_approved_application_activates_its_key() {
    let computer = Computer::new();
    let client = computer.applied();
    computer.decide(Decision::Approved);
    assert!(client.check().unwrap().is_licensed());
    assert_eq!(*computer.server.activated_keys.lock().unwrap(), [APPROVED_KEY.replace('-', "")]);
    assert!(client.status().is_licensed());
    let restarted = computer.start();
    assert!(restarted.status().is_licensed());
    assert_eq!(restarted.time_until_due(), Duration::from_secs(24 * 3600), "a licence's own daily check now");
}

#[test]
fn a_rejection_keeps_its_reason_until_set_aside() {
    let computer = Computer::new();
    let client = computer.applied();
    computer.decide(Decision::Rejected(Some("Only for schools")));
    let rejected = Outcome::ApplicationRejected { reason: Some("Only for schools".into()) };
    assert_eq!(client.check().unwrap(), rejected);
    assert_eq!(computer.start().status(), rejected, "kept across restarts");
    assert!(!client.needs_check(), "nothing left to ask");
    client.dismiss_application().unwrap();
    assert_eq!(client.status(), Outcome::NeedsActivation);
    assert_eq!(computer.start().status(), Outcome::NeedsActivation);
}

#[test]
fn a_withdrawn_or_deleted_application_counts_as_rejected() {
    for decision in [Decision::Withdrawn, Decision::Unknown] {
        let computer = Computer::new();
        let client = computer.applied();
        computer.decide(decision.clone());
        assert_eq!(client.check().unwrap(), Outcome::ApplicationRejected { reason: None }, "{decision:?}");
    }
}

#[test]
fn every_refusal_of_sending_says_why_and_keeps_nothing() {
    for (code, status, expected) in [
        ("application_pending", 409, Applied::AlreadyPending),
        ("invalid_request", 400, Applied::InvalidDetails),
        ("product_unavailable", 403, Applied::ProductUnavailable),
        ("rate_limited", 429, Applied::RateLimited { retry_after: Duration::from_secs(120) }),
        ("internal_error", 500, Applied::Unreachable),
    ] {
        let computer = Computer::new();
        computer.server.set_mode(Mode::Refuse(code, status));
        let client = computer.start();
        assert_eq!(client.apply(&ana()).unwrap(), expected, "{code}");
        assert_eq!(client.status(), Outcome::NeedsActivation, "{code}");
    }
    for mode in [Mode::Offline, Mode::Garbage] {
        let computer = Computer::new();
        computer.server.set_mode(mode.clone());
        assert_eq!(computer.start().apply(&ana()).unwrap(), Applied::Unreachable, "{mode:?}");
    }
}

#[test]
fn a_licensed_computer_sends_no_application() {
    let computer = Computer::new();
    let client = computer.start();
    assert!(client.activate("BRN-79YN-44XQ-W025-377Z").unwrap().is_licensed());
    let calls = computer.server.calls.load(Ordering::SeqCst);
    assert_eq!(client.apply(&ana()).unwrap(), Applied::AlreadyLicensed);
    assert_eq!(computer.server.calls.load(Ordering::SeqCst), calls, "nothing sent");
}

#[test]
fn an_application_replaces_a_revoked_licence() {
    let computer = Computer::new();
    let client = computer.start();
    assert!(client.activate("BRN-79YN-44XQ-W025-377Z").unwrap().is_licensed());
    computer.server.set_mode(Mode::Refuse("licence_revoked", 403));
    computer.advance(TimeDelta::days(1));
    assert_eq!(client.check().unwrap(), Outcome::Revoked);
    computer.server.set_mode(Mode::Normal);
    assert_eq!(client.apply(&ana()).unwrap(), Applied::Sent);
    assert_eq!(client.status(), Outcome::ApplicationPending);
}

#[test]
fn a_key_typed_while_waiting_ends_the_application() {
    let computer = Computer::new();
    let client = computer.applied();
    assert!(client.activate("BRN-79YN-44XQ-W025-377Z").unwrap().is_licensed());
    computer.advance(TimeDelta::days(1));
    computer.decide(Decision::Approved);
    assert!(client.check().unwrap().is_licensed());
    assert_eq!(computer.server.activated_keys.lock().unwrap().len(), 1, "the approved key wasn't activated too");
}

#[test]
fn a_question_that_fails_changes_nothing() {
    let computer = Computer::new();
    let client = computer.applied();
    computer.server.set_mode(Mode::Offline);
    assert!(matches!(client.check(), Err(Temporary::Unreachable(_))));
    computer.server.set_mode(Mode::Garbage);
    assert!(matches!(client.check(), Err(Temporary::Server(_))));
    computer.server.set_mode(Mode::Refuse("rate_limited", 429));
    assert!(matches!(client.check(), Err(Temporary::RateLimited(_))));
    assert_eq!(client.status(), Outcome::ApplicationPending);
    computer.server.set_mode(Mode::Normal);
    computer.decide(Decision::Approved);
    assert!(client.check().unwrap().is_licensed(), "asked again once the server answers");
}

#[test]
fn the_background_checker_activates_the_key_once_approved() {
    let computer = Computer::new();
    let client = Arc::new(computer.applied());
    computer.decide(Decision::Approved);
    let (tell, told) = mpsc::channel();
    let checks = Background::spawn(client.clone(), move |outcome| tell.send(outcome).unwrap()).unwrap();
    let outcome = told.recv_timeout(Duration::from_secs(5)).unwrap();
    checks.stop();
    assert!(outcome.is_licensed(), "{outcome:?}");
    assert!(client.status().is_licensed());
}

#[test]
fn setting_aside_nothing_is_fine() {
    let computer = Computer::new();
    let client = computer.start();
    client.dismiss_application().unwrap();
    assert_eq!(client.check_application().unwrap(), Outcome::NeedsActivation);
}

#[test]
fn the_key_page_address_names_the_product_and_stays_offline() {
    let computer = Computer::new();
    let client = computer.start();
    assert_eq!(client.get_key_url(), format!("http://fake/apply?product={PRODUCT}"));
    assert_eq!(computer.server.calls.load(Ordering::SeqCst), 0, "nothing was sent");
}

#[test]
fn the_key_page_address_encodes_the_product_code() {
    let computer = Computer::new();
    let config = Config::new("avalio lite&x=ñ", FakeServer::trusted_keys(), "2.0.0");
    let client = Client::new(
        config,
        Box::new(computer.store.clone()),
        Box::new(computer.server.clone()),
        Box::new(computer.clock.clone()),
    );
    assert_eq!(client.get_key_url(), "http://fake/apply?product=avalio%20lite%26x%3D%C3%B1");
}
