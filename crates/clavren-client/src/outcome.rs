// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:11.545654
//+ Revised: 	2026/11/03 04:54:11.545654
// -----------------------------------------------------------------------------

//! What the application is told about its licence. It shows its own (translated) message for each
//! outcome; none of them carries text meant for users.

use std::time::Duration;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The licence is valid. `offline_until` is when the cached token runs out without a check;
    /// an application may warn when it is under 5 days away.
    Licensed { licence_expires_at: Option<DateTime<Utc>>, offline_until: DateTime<Utc> },
    /// No licence on this computer yet: ask for a key.
    NeedsActivation,
    /// The key is malformed, unknown, or for another product: ask again.
    InvalidKey,
    /// The key is in use on another computer: show how to ask for a reset.
    AlreadyActivatedElsewhere,
    /// The licence was revoked, or this computer was reset: lock, show the support contact.
    Revoked,
    /// The licence has expired (at `at`, when known): lock, show the support contact.
    Expired { at: Option<DateTime<Utc>> },
    /// The product was withdrawn: lock.
    ProductUnavailable,
    /// The cached token ran out (30 days offline) or the clock was set back: ask to connect.
    NeedsCheck,
    /// Too many attempts: wait this long.
    RateLimited { retry_after: Duration },
    /// The licence server couldn't be reached (activating needs it): try again later.
    Unreachable,
}

impl Outcome {
    /// Whether the application may run.
    pub fn is_licensed(&self) -> bool {
        matches!(self, Self::Licensed { .. })
    }
}

/// Why the last online check couldn't decide anything. While the cached token is valid, these
/// change nothing; the background checker retries with backoff.
#[derive(Debug, thiserror::Error)]
pub enum Temporary {
    #[error("the licence server couldn't be reached: {0}")]
    Unreachable(String),
    #[error("the licence server asked to wait {0:?}")]
    RateLimited(Duration),
    #[error("the licence server answered unexpectedly: {0}")]
    Server(String),
    #[error("the licence state couldn't be saved: {0}")]
    Storage(#[from] crate::store::StoreError),
}
