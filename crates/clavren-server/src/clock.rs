// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.333823
//+ Revised: 	2026/11/03 04:54:22.209007
// -----------------------------------------------------------------------------

//! The current time, behind a trait so tests can move it: TOTP codes, session expiry and rate
//! limits all depend on it.

use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A clock that only moves when told to.
pub struct ManualClock(Mutex<DateTime<Utc>>);

impl ManualClock {
    pub fn new(start: DateTime<Utc>) -> Self {
        Self(Mutex::new(start))
    }

    pub fn advance(&self, by: Duration) {
        let mut now = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *now += by;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_clock_moves_only_when_advanced() {
        let start = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let clock = ManualClock::new(start);
        assert_eq!(clock.now(), start);
        clock.advance(Duration::from_secs(90));
        assert_eq!(clock.now(), start + Duration::from_secs(90));
    }

    #[test]
    fn the_system_clock_is_close_to_now() {
        assert!((SystemClock.now() - Utc::now()).num_seconds().abs() < 5);
    }
}
