// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:29.959184
//+ Revised: 	2026/11/14 11:36:29.959184
// -----------------------------------------------------------------------------

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
fn an_attempt_counts_against_every_key_or_none() {
    let limiter = FailureLimiter::new(2, WINDOW);
    let keys = ["ip".to_string(), "user".to_string()];
    limiter.record_failure("user", start());
    limiter.record_failure("user", start());
    assert_eq!(limiter.attempt(&keys, start()), Err(WINDOW));
    assert_eq!(limiter.tracked_keys(), 1, "the address wasn't counted for a refused attempt");
    limiter.clear("user");
    assert!(limiter.attempt(&keys, start()).is_ok());
    assert!(limiter.attempt(&keys, start()).is_ok());
    assert!(limiter.attempt(&keys, start()).is_err(), "counted before the outcome is known");
}

#[test]
fn forgiving_takes_back_one_attempt() {
    let limiter = FailureLimiter::new(1, WINDOW);
    let keys = ["key".to_string()];
    assert!(limiter.attempt(&keys, start()).is_ok());
    limiter.forgive("key", start());
    assert_eq!(limiter.tracked_keys(), 0);
    assert!(limiter.attempt(&keys, start()).is_ok());
    limiter.forgive("key", start() + WINDOW);
    assert!(limiter.check("key", start()).is_err(), "only an attempt made at that time is taken back");
    limiter.forgive("unknown", start());
}

/// Regression: the login checked the count, waited for Argon2, then counted; requests sent in
/// parallel all passed the check, so the limit didn't bind (and each ran Argon2).
#[test]
fn parallel_attempts_never_pass_the_limit() {
    let limiter = std::sync::Arc::new(FailureLimiter::new(5, WINDOW));
    let passed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let threads: Vec<_> = (0..50)
        .map(|_| {
            let (limiter, passed) = (limiter.clone(), passed.clone());
            std::thread::spawn(move || {
                if limiter.attempt(&["key".to_string()], start()).is_ok() {
                    passed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            })
        })
        .collect();
    threads.into_iter().for_each(|thread| thread.join().unwrap());
    assert_eq!(passed.load(std::sync::atomic::Ordering::SeqCst), 5);
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
