// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:09.222740
//+ Revised: 	2026/11/09 22:30:52.782718
// -----------------------------------------------------------------------------

//! Checking in the background: a thread that checks whenever a check is due, retries a failed one
//! after 1, 5, 15, then every 60 minutes, and tells the application when the outcome changes. It
//! never touches the UI thread; the application decides what to do with each outcome.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::client::Client;
use crate::outcome::{Outcome, Temporary};

pub const BACKOFF: [Duration; 4] =
    [Duration::from_secs(60), Duration::from_secs(5 * 60), Duration::from_secs(15 * 60), Duration::from_secs(60 * 60)];
/// The loop looks again at least this often, so a check that became due is never missed for long
/// and the time seen is saved regularly.
const LONGEST_SLEEP: Duration = Duration::from_secs(10 * 60);

/// The running checker. Stops (and waits for its thread) when stopped or dropped.
pub struct Background {
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Background {
    /// Starts checking. `on_change` runs on the checker's thread whenever an online check gives a
    /// different kind of outcome from the last one reported (starting from [`Client::status`]).
    pub fn spawn(client: Arc<Client>, on_change: impl Fn(Outcome) + Send + 'static) -> std::io::Result<Self> {
        let (stop, stopped) = mpsc::channel();
        let thread = std::thread::Builder::new().name("licence-checks".into()).spawn(move || {
            let mut reported = client.status();
            let mut retry = Retry::default();
            loop {
                let wait = if let Some(wait) = retry.pending(Instant::now()) {
                    wait
                } else if client.needs_check() {
                    match client.check() {
                        Ok(outcome) => {
                            retry = Retry::default();
                            // a renewal moves `offline_until`; only a different kind of outcome is news
                            if std::mem::discriminant(&outcome) != std::mem::discriminant(&reported) {
                                reported = outcome.clone();
                                if catch_unwind(AssertUnwindSafe(|| on_change(outcome))).is_err() {
                                    log::error!("the application's licence callback panicked; checks go on");
                                }
                            }
                            // never again at once: a check that answered but left nothing to run
                            // on would otherwise ask the server in a tight loop
                            let due = client.time_until_due();
                            if due.is_zero() { retry.wait_at_least(Instant::now(), BACKOFF[0]) } else { due }
                        }
                        Err(problem) => {
                            log::warn!("licence check failed, retrying later: {problem}");
                            retry.failed(Instant::now(), &problem)
                        }
                    }
                } else {
                    client.time_until_due()
                };
                save_time_seen(&client);
                match stopped.recv_timeout(wait.min(LONGEST_SLEEP)) {
                    Err(RecvTimeoutError::Timeout) => continue,
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            save_time_seen(&client);
            log::info!("licence checks stopped");
        })?;
        Ok(Self { stop: Some(stop), thread: Some(thread) })
    }

    /// Stops the checker and waits for its thread, which may be finishing a check: up to two
    /// requests, each within the transport's timeout (30 seconds in all with [`HttpTransport`]).
    /// Dropping it does the same, so stop it off the UI thread when that matters.
    ///
    /// [`HttpTransport`]: crate::transport::HttpTransport
    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        if let Some(stop) = self.stop.take() {
            // the thread may have ended already; then there is nothing to tell
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            log::error!("the licence check thread panicked");
        }
    }
}

/// When the next check may go out after a failed one. The loop wakes up at least every
/// [`LONGEST_SLEEP`], so the backoff is kept here rather than in how long it sleeps.
#[derive(Default)]
struct Retry {
    failures: usize,
    not_before: Option<Instant>,
}

impl Retry {
    /// How long until a check may go out again, or `None` when it may now.
    fn pending(&self, now: Instant) -> Option<Duration> {
        self.not_before.filter(|at| now < *at).map(|at| at - now)
    }

    /// Records a failed check; the wait grows along [`BACKOFF`], or is what the server asked for.
    fn failed(&mut self, now: Instant, problem: &Temporary) -> Duration {
        let backoff = BACKOFF[self.failures.min(BACKOFF.len() - 1)];
        self.failures += 1;
        let wait = match problem {
            Temporary::RateLimited(retry_after) => backoff.max(*retry_after),
            _ => backoff,
        };
        self.wait_at_least(now, wait)
    }

    fn wait_at_least(&mut self, now: Instant, wait: Duration) -> Duration {
        self.not_before = Some(now + wait);
        wait
    }
}

fn save_time_seen(client: &Client) {
    if let Err(error) = client.save_time_seen() {
        log::error!("couldn't save the time seen: {error}");
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        self.shut_down();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    #[test]
    fn failed_checks_back_off_1_5_15_then_60_minutes() {
        let (mut retry, start) = (Retry::default(), Instant::now());
        let waits: Vec<_> =
            (0..5).map(|_| retry.failed(start, &Temporary::Unreachable("offline".into())) / 60).collect();
        assert_eq!(waits, [Duration::from_secs(1), Duration::from_secs(5), Duration::from_secs(15), MINUTE, MINUTE]);
    }

    /// Regression: the loop wakes up every 10 minutes, and each wake-up used to check again, so
    /// the 15 and 60 minute steps (and a server's Retry-After) were never kept.
    #[test]
    fn no_check_goes_out_before_the_backoff_ends() {
        let (mut retry, start) = (Retry::default(), Instant::now());
        retry.failed(start, &Temporary::RateLimited(60 * MINUTE));
        assert_eq!(retry.pending(start + LONGEST_SLEEP), Some(50 * MINUTE));
        assert_eq!(retry.pending(start + 59 * MINUTE), Some(MINUTE));
        assert_eq!(retry.pending(start + 60 * MINUTE), None);
    }

    #[test]
    fn a_fresh_retry_never_waits() {
        assert_eq!(Retry::default().pending(Instant::now()), None);
    }
}
