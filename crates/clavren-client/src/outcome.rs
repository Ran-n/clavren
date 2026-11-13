// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:11.545654
//+ Revised: 	2026/11/11 16:32:49.856298
// -----------------------------------------------------------------------------

//! What the application is told about its licence, and about sending a licence application. It
//! shows its own (translated) message for each; the only text meant for users is the reason an
//! administrator gave for rejecting an application.

use std::time::Duration;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The licence is valid. `offline_until` is when the cached token runs out without a check;
    /// an application may warn when it is under 5 days away. `test` is a test licence (for trying
    /// the application, not a sale): an application may say so, or limit what it does.
    Licensed { licence_expires_at: Option<DateTime<Utc>>, offline_until: DateTime<Utc>, test: bool },
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
    /// A licence application from this computer waits for the administrator. The background
    /// checker asks every 15 minutes and activates the key once it is approved. The application
    /// may also offer to type a key instead.
    ApplicationPending,
    /// The licence application was rejected, or its licence withdrawn since; `reason` is the
    /// administrator's, written for the applicant. Show it, then ask for a key or offer to apply
    /// again ([`crate::Client::dismiss_application`] goes back to asking for a key).
    ApplicationRejected { reason: Option<String> },
}

/// What came of sending a licence application ([`crate::Client::apply`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// Sent: [`crate::Client::status`] says [`Outcome::ApplicationPending`] until it is decided.
    Sent,
    /// This address already has an application waiting for this product (sent from another
    /// computer, or from the web form).
    AlreadyPending,
    /// The name or address was refused: empty, too long, or not an address.
    InvalidDetails,
    /// The product takes no applications (now).
    ProductUnavailable,
    /// This computer already runs on a valid licence: nothing was sent.
    AlreadyLicensed,
    /// Too many attempts from this network: wait this long.
    RateLimited { retry_after: Duration },
    /// The licence server couldn't be reached: try again later.
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
