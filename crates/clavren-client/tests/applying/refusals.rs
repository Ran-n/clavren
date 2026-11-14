// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:21.656100
//+ Revised: 	2026/11/14 17:01:17.013257
// -----------------------------------------------------------------------------

//! Applications that are refused, not sent, replaced or left as they were.

use super::*;

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

/// What the status endpoint answers for an application in `status`, with `key`.
fn status_answer(status: &str, key: Option<&str>) -> Scripted {
    let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
    let body = serde_json::json!({
        "application": APPLICATION, "product": PRODUCT, "status": status, "created_at": now, "decided_at": now,
        "reason": null, "key": key, "licence_expires_at": null, "server_time": now,
    });
    Scripted::Answer(200, body)
}

#[test]
fn an_application_taken_without_its_secret_counts_as_not_sent() {
    let computer = Computer::new();
    computer.server.script("/api/v1/applications", Scripted::Answer(201, serde_json::Value::Null));
    let client = computer.start();
    assert_eq!(client.apply(&ana()).unwrap(), Applied::Unreachable);
    assert_eq!(client.status(), Outcome::NeedsActivation, "nothing waits: there is no secret to ask with");
}

#[test]
fn answers_about_an_application_that_make_no_sense_change_nothing() {
    for (answer, why) in [
        (Scripted::Answer(500, serde_json::json!({ "error": "internal_error", "message": "no" })), "a server error"),
        (status_answer("approved", None), "an approval without its key"),
    ] {
        let computer = Computer::new();
        let client = computer.applied();
        computer.server.script("/applications/status", answer);
        assert!(matches!(client.check(), Err(Temporary::Server(_))), "{why}");
        assert_eq!(client.status(), Outcome::ApplicationPending, "{why}");
    }
}

#[test]
fn an_approved_key_that_cannot_be_activated_yet_is_tried_again() {
    let limited = Scripted::Answer(429, serde_json::json!({ "error": "rate_limited", "message": "no" }));
    for (activating, unreachable) in [(Scripted::Offline, true), (limited, false)] {
        let computer = Computer::new();
        let client = computer.applied();
        computer.decide(Decision::Approved);
        computer.server.script("/api/v1/activations", activating);
        match client.check() {
            Err(Temporary::Unreachable(_)) => assert!(unreachable),
            Err(Temporary::RateLimited(wait)) => assert_eq!((unreachable, wait), (false, Duration::from_secs(120))),
            other => panic!("{other:?}"),
        }
        assert_eq!(client.status(), Outcome::ApplicationPending, "still waiting, so the next check tries again");
    }
}

#[test]
fn an_approved_key_the_server_then_refuses_ends_the_application() {
    let computer = Computer::new();
    let client = computer.applied();
    computer.decide(Decision::Approved);
    let revoked = serde_json::json!({ "error": "licence_revoked", "message": "no" });
    computer.server.script("/api/v1/activations", Scripted::Answer(403, revoked));
    assert_eq!(client.check().unwrap(), Outcome::Revoked);
    assert_eq!(client.status(), Outcome::NeedsActivation, "the application is over: asking again would say the same");
}
