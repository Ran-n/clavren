// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:48:57.550384
//+ Revised: 	2026/11/14 11:48:57.550384
// -----------------------------------------------------------------------------

//! When the next online check is due.

use crate::client::Client;
use crate::state::Application;
use chrono::{DateTime, TimeDelta, Utc};
use std::time::Duration;

impl Client {
    /// Whether an online check is due: the last successful one is older than the interval, or
    /// there is nothing valid to run on, or a waiting licence application is due to be asked
    /// about. A revoked licence is never checked again.
    pub fn needs_check(&self) -> bool {
        self.time_until_due().is_zero()
    }

    /// How long until the next online check is due (zero when it is due now).
    pub fn time_until_due(&self) -> Duration {
        let now = self.clock.now();
        let state = self.lock();
        if state.activation.is_none() {
            return match &state.application {
                Some(Application::Pending { last_check: Some(last), .. }) => {
                    until(*last, self.config.application_check_interval, now)
                }
                Some(Application::Pending { last_check: None, .. }) => Duration::ZERO,
                _ => Duration::MAX,
            };
        }
        let licensed = self.offline_outcome(&state, now).is_licensed();
        match state.last_check {
            // a refusal is an answer too: ask again at the usual pace (an expiry may be extended)
            Some(last) if licensed || state.refusal.is_some() => until(last, self.config.check_interval, now),
            _ => Duration::ZERO,
        }
    }
}

/// How long from `now` until `interval` has passed since `last` (zero when it has).
fn until(last: DateTime<Utc>, interval: Duration, now: DateTime<Utc>) -> Duration {
    // an interval too long for the calendar means "not again"
    let Some(due) = TimeDelta::from_std(interval).ok().and_then(|interval| last.checked_add_signed(interval)) else {
        return Duration::MAX;
    };
    (due - now).to_std().unwrap_or(Duration::ZERO)
}
