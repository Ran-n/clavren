// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.214266
//+ Revised: 	2026/11/14 11:36:30.061197
// -----------------------------------------------------------------------------

//! Counting failures per key (a client address, a username) over a sliding window, in memory.
//! Clavren runs as a single instance, so a restart clearing the counts is acceptable.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};

pub struct FailureLimiter {
    max_failures: u32,
    window: Duration,
    failures: Mutex<HashMap<String, VecDeque<DateTime<Utc>>>>,
}

impl FailureLimiter {
    pub fn new(max_failures: u32, window: Duration) -> Self {
        Self { max_failures, window, failures: Mutex::new(HashMap::new()) }
    }

    /// `Err(wait)` while `key` has used up its failures; `wait` is how long until the oldest one
    /// leaves the window. Requests count with [`Self::attempt`], which checks and counts at once.
    #[cfg(test)]
    pub fn check(&self, key: &str, now: DateTime<Utc>) -> Result<(), Duration> {
        let mut failures = self.lock();
        failures.get_mut(key).map_or(Ok(()), |times| self.allowance(times, now))
    }

    pub fn record_failure(&self, key: &str, now: DateTime<Utc>) {
        self.lock().entry(key.to_string()).or_default().push_back(now);
    }

    /// Counts an attempt against every key at once, unless one of them has used up its allowance
    /// (then nothing is counted). Checking and counting happen under one lock, so requests sent
    /// in parallel can't all slip under the limit before any of them is counted. An attempt that
    /// turns out well is taken back with [`Self::forgive`].
    pub fn attempt(&self, keys: &[String], now: DateTime<Utc>) -> Result<(), Duration> {
        let mut failures = self.lock();
        for key in keys {
            if let Some(times) = failures.get_mut(key) {
                self.allowance(times, now)?;
            }
        }
        for key in keys {
            failures.entry(key.clone()).or_default().push_back(now);
        }
        Ok(())
    }

    /// Counts one request for `key`, unless `key` already used up its allowance.
    pub fn hit(&self, key: &str, now: DateTime<Utc>) -> Result<(), Duration> {
        self.attempt(&[key.to_string()], now)
    }

    /// Takes back one attempt counted for `key` at `at` by [`Self::attempt`].
    pub fn forgive(&self, key: &str, at: DateTime<Utc>) {
        let mut failures = self.lock();
        let Some(times) = failures.get_mut(key) else { return };
        if let Some(position) = times.iter().rposition(|&time| time == at) {
            times.remove(position);
        }
        if times.is_empty() {
            failures.remove(key);
        }
    }

    pub fn clear(&self, key: &str) {
        self.lock().remove(key);
    }

    /// Drops keys with no failure left in the window, so the map doesn't grow with every address
    /// that ever failed once. Run by the maintenance task.
    pub fn prune(&self, now: DateTime<Utc>) {
        let mut failures = self.lock();
        failures.retain(|_, times| {
            self.forget_old(times, now);
            !times.is_empty()
        });
    }

    pub fn tracked_keys(&self) -> usize {
        self.lock().len()
    }

    fn allowance(&self, times: &mut VecDeque<DateTime<Utc>>, now: DateTime<Utc>) -> Result<(), Duration> {
        self.forget_old(times, now);
        if times.len() < self.max_failures as usize {
            return Ok(());
        }
        let oldest = times.front().copied().unwrap_or(now);
        Err((oldest + self.window - now).to_std().unwrap_or(Duration::ZERO))
    }

    fn forget_old(&self, times: &mut VecDeque<DateTime<Utc>>, now: DateTime<Utc>) {
        while times.front().is_some_and(|&time| time + self.window <= now) {
            times.pop_front();
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, VecDeque<DateTime<Utc>>>> {
        // a panic while holding the lock leaves only counters behind, which are still usable
        self.failures.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The public API's limits, per hour: activations per client address, challenges and checks per
/// client address, checks per activation, licence applications sent per client address, and
/// questions about how an application went per client address.
pub struct PublicLimits {
    pub activations: FailureLimiter,
    pub checks_per_ip: FailureLimiter,
    pub checks_per_activation: FailureLimiter,
    pub applications: FailureLimiter,
    pub application_status: FailureLimiter,
}

impl Default for PublicLimits {
    fn default() -> Self {
        let hour = Duration::from_secs(3600);
        Self {
            activations: FailureLimiter::new(10, hour),
            checks_per_ip: FailureLimiter::new(120, hour),
            checks_per_activation: FailureLimiter::new(30, hour),
            applications: FailureLimiter::new(5, hour),
            application_status: FailureLimiter::new(120, hour),
        }
    }
}

impl PublicLimits {
    pub fn prune(&self, now: DateTime<Utc>) {
        self.activations.prune(now);
        self.checks_per_ip.prune(now);
        self.checks_per_activation.prune(now);
        self.applications.prune(now);
        self.application_status.prune(now);
    }
}

#[cfg(test)]
mod tests;
