// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:22.946817
//+ Revised: 	2026/11/14 12:05:22.946817
// -----------------------------------------------------------------------------

//! Tokens and answers the client must not trust.

use super::*;

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
fn a_token_from_another_device_is_not_trusted() {
    let computer = Computer::new();
    computer.activated();
    let mut saved: serde_json::Value = serde_json::from_slice(&computer.store.load().unwrap().unwrap()).unwrap();
    saved["device_key"] = serde_json::json!(clavren_protocol::encoding::encode(&[9; 32]));
    computer.store.save(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(computer.start().status(), Outcome::NeedsCheck);
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
