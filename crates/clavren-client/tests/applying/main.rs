// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:48.386270
//+ Revised: 	2026/11/14 12:05:21.872151
// -----------------------------------------------------------------------------

//! Applying for a licence from the application: sending one, waiting (across restarts), the key
//! activated by itself once approved, a rejection with its reason, every refusal of the sending,
//! and the background checker asking until it is decided.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

#[path = "../fake_server/mod.rs"]
mod fake_server;
mod key_page;
mod refusals;

use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use chrono::{DateTime, TimeDelta};
use clavren_client::clock::{Clock as _, ManualClock};
use clavren_client::store::MemoryStore;
use clavren_client::{Applicant, Applied, Background, Client, Config, Outcome, Temporary};
use fake_server::{APPLICATION, APPROVED_KEY, Decision, FakeServer, Mode, PRODUCT, Scripted};

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

/// Regression: applying again replaced the waiting application's secret, so its approval was
/// never collected on this computer.
#[test]
fn applying_again_while_one_waits_sends_nothing() {
    let computer = Computer::new();
    let client = computer.applied();
    let calls = computer.server.calls.load(Ordering::SeqCst);
    let again = Applicant { email: "other@example.com".into(), ..ana() };
    assert_eq!(client.apply(&again).unwrap(), Applied::AlreadyPending);
    assert_eq!(computer.server.calls.load(Ordering::SeqCst), calls, "nothing was sent");
    assert_eq!(computer.server.applications.lock().unwrap().len(), 1);
    client.dismiss_application().unwrap();
    assert_eq!(client.apply(&again).unwrap(), Applied::Sent, "set aside, it can be sent again");
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
