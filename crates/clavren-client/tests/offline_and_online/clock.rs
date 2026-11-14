// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:22.546470
//+ Revised: 	2026/11/14 12:05:22.546470
// -----------------------------------------------------------------------------

//! The clock set back, or simply wrong: what the client makes of the time.

use super::*;

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
