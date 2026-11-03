// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.214266
//+ Revised: 	2026/11/03 04:54:22.149100
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
    /// leaves the window.
    pub fn check(&self, key: &str, now: DateTime<Utc>) -> Result<(), Duration> {
        let mut failures = self.lock();
        let Some(times) = failures.get_mut(key) else { return Ok(()) };
        self.forget_old(times, now);
        if times.len() < self.max_failures as usize {
            return Ok(());
        }
        let oldest = times.front().copied().unwrap_or(now);
        Err((oldest + self.window - now).to_std().unwrap_or(Duration::ZERO))
    }

    pub fn record_failure(&self, key: &str, now: DateTime<Utc>) {
        self.lock().entry(key.to_string()).or_default().push_back(now);
    }

    /// Counts one request for `key`, unless `key` already used up its allowance.
    pub fn hit(&self, key: &str, now: DateTime<Utc>) -> Result<(), Duration> {
        self.check(key, now)?;
        self.record_failure(key, now);
        Ok(())
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
/// client address, and checks per activation.
pub struct PublicLimits {
    pub activations: FailureLimiter,
    pub checks_per_ip: FailureLimiter,
    pub checks_per_activation: FailureLimiter,
}

impl Default for PublicLimits {
    fn default() -> Self {
        let hour = Duration::from_secs(3600);
        Self {
            activations: FailureLimiter::new(10, hour),
            checks_per_ip: FailureLimiter::new(120, hour),
            checks_per_activation: FailureLimiter::new(30, hour),
        }
    }
}

impl PublicLimits {
    pub fn prune(&self, now: DateTime<Utc>) {
        self.activations.prune(now);
        self.checks_per_ip.prune(now);
        self.checks_per_activation.prune(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(900);

    fn start() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn fail(limiter: &FailureLimiter, times: u32, at: DateTime<Utc>) {
        (0..times).for_each(|_| limiter.record_failure("key", at));
    }

    #[test]
    fn failures_below_the_limit_are_allowed() {
        let limiter = FailureLimiter::new(5, WINDOW);
        fail(&limiter, 4, start());
        assert!(limiter.check("key", start()).is_ok());
    }

    #[test]
    fn hits_count_every_request_up_to_the_limit() {
        let limiter = FailureLimiter::new(2, WINDOW);
        assert!(limiter.hit("key", start()).is_ok());
        assert!(limiter.hit("key", start()).is_ok());
        assert_eq!(limiter.hit("key", start()), Err(WINDOW));
        assert!(limiter.hit("key", start() + WINDOW).is_ok(), "a refused hit isn't counted");
    }

    #[test]
    fn reaching_the_limit_blocks_until_the_oldest_failure_expires() {
        let limiter = FailureLimiter::new(5, WINDOW);
        fail(&limiter, 1, start());
        fail(&limiter, 4, start() + Duration::from_secs(300));
        assert_eq!(limiter.check("key", start() + Duration::from_secs(600)), Err(Duration::from_secs(300)));
        assert!(limiter.check("key", start() + WINDOW).is_ok());
    }

    #[test]
    fn keys_are_independent() {
        let limiter = FailureLimiter::new(1, WINDOW);
        fail(&limiter, 1, start());
        assert!(limiter.check("key", start()).is_err());
        assert!(limiter.check("other", start()).is_ok());
    }

    #[test]
    fn clearing_forgets_a_key() {
        let limiter = FailureLimiter::new(1, WINDOW);
        fail(&limiter, 1, start());
        limiter.clear("key");
        assert!(limiter.check("key", start()).is_ok());
    }

    #[test]
    fn pruning_drops_only_keys_without_recent_failures() {
        let limiter = FailureLimiter::new(5, WINDOW);
        fail(&limiter, 1, start());
        limiter.record_failure("recent", start() + WINDOW);
        limiter.prune(start() + WINDOW);
        assert_eq!(limiter.tracked_keys(), 1);
        assert!(limiter.check("recent", start() + WINDOW).is_ok());
    }
}
