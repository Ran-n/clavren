// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.392152
//+ Revised: 	2026/11/11 17:53:38.139273
// -----------------------------------------------------------------------------

//! The hourly housekeeping task: deletes sessions and challenges that can never be used again and
//! old check counts, and forgets old rate-limit counts. It runs once at startup, then every hour,
//! and stops on shutdown. Licence applications are history and are never cleaned up.

use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::activations;
use crate::admin_auth::sessions;
use crate::http::AppState;

pub const INTERVAL: Duration = Duration::from_secs(3600);

#[derive(Debug, PartialEq, Eq)]
pub struct Cleaned {
    pub sessions: u64,
    pub challenges: u64,
    /// Hourly check counts older than 30 days.
    pub check_counts: u64,
}

pub async fn run_once(state: &AppState) -> Result<Cleaned, sqlx::Error> {
    let now = state.clock.now();
    let sessions = sessions::delete_finished(&state.pool, now, &state.settings.admin).await?;
    let challenges = activations::repository::delete_finished_challenges(&state.pool, now).await?;
    let check_counts = activations::repository::delete_old_check_counts(&state.pool, now).await?;
    state.login_failures.prune(now);
    state.public_limits.prune(now);
    Ok(Cleaned { sessions, challenges, check_counts })
}

pub fn spawn(state: AppState, stop: CancellationToken, interval: Duration) -> JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!("maintenance task started");
        let mut ticks = tokio::time::interval(interval);
        loop {
            tokio::select! {
                () = stop.cancelled() => break,
                _ = ticks.tick() => match run_once(&state).await {
                    Ok(cleaned) => tracing::info!(
                        sessions_deleted = cleaned.sessions,
                        challenges_deleted = cleaned.challenges,
                        check_counts_deleted = cleaned.check_counts,
                        "maintenance done"
                    ),
                    // logged and retried at the next tick; one failed run loses nothing
                    Err(error) => tracing::error!(%error, "maintenance failed"),
                },
            }
        }
        tracing::info!("maintenance task stopped");
    })
}
