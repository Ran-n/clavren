// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:09.222740
//+ Revised: 	2026/11/03 04:54:09.222740
// -----------------------------------------------------------------------------

//! Checking in the background: a thread that checks whenever a check is due, retries a failed one
//! after 1, 5, 15, then every 60 minutes, and tells the application when the outcome changes. It
//! never touches the UI thread; the application decides what to do with each outcome.

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::client::Client;
use crate::outcome::{Outcome, Temporary};

pub const BACKOFF: [Duration; 4] =
    [Duration::from_secs(60), Duration::from_secs(5 * 60), Duration::from_secs(15 * 60), Duration::from_secs(60 * 60)];
/// The loop looks again at least this often, so a check that became due is never missed for long.
const LONGEST_SLEEP: Duration = Duration::from_secs(60 * 60);

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
            let mut failures = 0usize;
            loop {
                let wait = if client.needs_check() {
                    match client.check() {
                        Ok(outcome) => {
                            failures = 0;
                            // a renewal moves `offline_until`; only a different kind of outcome is news
                            if std::mem::discriminant(&outcome) != std::mem::discriminant(&reported) {
                                reported = outcome.clone();
                                on_change(outcome);
                            }
                            client.time_until_due()
                        }
                        Err(problem) => {
                            log::warn!("licence check failed, retrying later: {problem}");
                            let backoff = BACKOFF[failures.min(BACKOFF.len() - 1)];
                            failures += 1;
                            match problem {
                                Temporary::RateLimited(retry_after) => backoff.max(retry_after),
                                _ => backoff,
                            }
                        }
                    }
                } else {
                    client.time_until_due()
                };
                match stopped.recv_timeout(wait.min(LONGEST_SLEEP)) {
                    Err(RecvTimeoutError::Timeout) => continue,
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            log::info!("licence checks stopped");
        })?;
        Ok(Self { stop: Some(stop), thread: Some(thread) })
    }

    /// Stops the checker and waits for its thread, which may be finishing a request (up to the
    /// transport's timeout).
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

impl Drop for Background {
    fn drop(&mut self) {
        self.shut_down();
    }
}
