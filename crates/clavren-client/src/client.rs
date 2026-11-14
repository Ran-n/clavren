// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:10.584826
//+ Revised: 	2026/11/14 11:48:57.790856
// -----------------------------------------------------------------------------

//! The licence of one application on this computer: its offline status, and when to check online.

use std::sync::{Mutex, MutexGuard, PoisonError};

use chrono::{DateTime, TimeDelta, Utc};
use clavren_protocol::check::device_hash;
use clavren_protocol::encoding;
use clavren_protocol::token::{self, Claims};
use ed25519_dalek::SigningKey;

use crate::clock::Clock;
use crate::config::Config;
use crate::outcome::Outcome;
use crate::persist::{Seen, Stored, parse_state};
use crate::state::{Application, Refusal, State};
use crate::store::{SecretStore, StoreError};
use crate::transport::Transport;

/// A clock more than this far behind the latest time seen, or set back more than this in all (many
/// small steps count together) since the last check, counts as set back (more than normal
/// adjustments), so the cached token isn't trusted until an online check.
pub const ROLLBACK_TOLERANCE: TimeDelta = TimeDelta::hours(1);
pub struct Client {
    pub(crate) config: Config,
    pub(crate) store: Box<dyn SecretStore>,
    pub(crate) transport: Box<dyn Transport>,
    pub(crate) clock: Box<dyn Clock>,
    pub(crate) state: Mutex<State>,
    /// Why the saved state couldn't be read at start, when it exists but couldn't be read (not
    /// when it is missing, damaged or another user's): then nothing is saved over it.
    pub(crate) unreadable: Option<String>,
    /// The latest time seen and the total set back, as they were last saved.
    pub(crate) saved_seen: Mutex<Seen>,
    /// What the store held when this program last loaded or saved it.
    pub(crate) stored: Mutex<Stored>,
}

/// `value` percent-encoded for a query string: everything but letters, digits and `-._~`.
fn query_value(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => char::from(byte).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

impl Client {
    /// Loads the saved state. State that is damaged or belongs to another Windows account counts
    /// as none: the application asks for its key again. A file that exists but can't be read
    /// (another program has it open) also starts empty, but is never overwritten: saving fails
    /// until the application starts again.
    pub fn new(
        config: Config,
        store: Box<dyn SecretStore>,
        transport: Box<dyn Transport>,
        clock: Box<dyn Clock>,
    ) -> Self {
        let mut unreadable = None;
        let (state, stored) = match store.load() {
            Ok(Some(bytes)) => (parse_state(&bytes), Stored::Known(Some(bytes))),
            Ok(None) => (State::default(), Stored::Known(None)),
            Err(error @ StoreError::Io { .. }) => {
                log::error!("the saved licence state can't be read ({error}); it won't be overwritten");
                unreadable = Some(error.to_string());
                (State::default(), Stored::Unknown)
            }
            Err(error) => {
                log::warn!("the saved licence state can't be read ({error}); starting over");
                (State::default(), Stored::Unknown)
            }
        };
        let saved_seen = Mutex::new(Seen::of(&state));
        let stored = Mutex::new(stored);
        Self { config, store, transport, clock, state: Mutex::new(state), unreadable, saved_seen, stored }
    }

    /// The address of this product's "how to get a key" page on the licence server,
    /// `<server>/apply?product=<code>`, where the administrator says how to get a key and, when
    /// applications are open, people apply. Link it from the key prompt as **How to get a key**
    /// and open it in the default browser, instead of writing instructions into the application:
    /// they change on the server for every installed copy at once. Never goes online.
    pub fn get_key_url(&self) -> String {
        self.transport.page_url(&format!("/apply?product={}", query_value(&self.config.product)))
    }

    /// The licence as far as this computer knows, without going online or touching the disk, so
    /// the UI thread can ask as often as it likes. The time it saw is kept in memory; the
    /// background checker saves it ([`Self::save_time_seen`]).
    pub fn status(&self) -> Outcome {
        // the time is read under the lock: two threads asking at once never record a step back
        let mut state = self.lock();
        let now = self.clock.now();
        let outcome = self.offline_outcome(&state, now);
        // every time seen counts, locked or not: a clock set back while the licence is locked
        // must not be waited out
        state.saw(now);
        outcome
    }

    pub(crate) fn offline_outcome(&self, state: &State, now: DateTime<Utc>) -> Outcome {
        if let Some(refusal) = state.refusal {
            return refusal_outcome(refusal);
        }
        let Some(token) = state.token.as_deref() else {
            return match (&state.activation, &state.application) {
                (Some(_), _) => Outcome::NeedsCheck,
                (None, Some(Application::Pending { .. })) => Outcome::ApplicationPending,
                (None, Some(Application::Rejected { reason })) => {
                    Outcome::ApplicationRejected { reason: reason.clone() }
                }
                (None, None) => Outcome::NeedsActivation,
            };
        };
        let behind_latest = state
            .latest_seen
            .is_some_and(|latest| latest.checked_sub_signed(ROLLBACK_TOLERANCE).is_none_or(|earliest| now < earliest));
        if behind_latest || state.rewound_by(now) > ROLLBACK_TOLERANCE.num_seconds() {
            log::warn!("the clock is behind the latest time seen; the licence must be checked online");
            return Outcome::NeedsCheck;
        }
        let claims = match self.usable_claims(state, token, server_now(state, now)) {
            Ok(claims) => claims,
            Err(problem) => {
                log::info!("the cached licence token can't be used: {problem}");
                return Outcome::NeedsCheck;
            }
        };
        Outcome::Licensed {
            licence_expires_at: claims.lic_exp.and_then(|at| DateTime::from_timestamp(at, 0)),
            offline_until: DateTime::from_timestamp(claims.exp, 0).unwrap_or(now),
            test: claims.test,
        }
    }

    /// The claims of `token` if it can be run on: signed by a trusted key for this product, not
    /// expired at `server_now`, and for this activation on this device.
    pub(crate) fn usable_claims(
        &self,
        state: &State,
        token: &str,
        server_now: DateTime<Utc>,
    ) -> Result<Claims, String> {
        let claims = token::verify(token, &self.config.trusted_keys, &self.config.product, server_now.timestamp())
            .map_err(|error| error.to_string())?;
        if Some(&claims.dev) != device_public_key(state).map(|key| device_hash(&key)).as_ref() {
            return Err("it belongs to another device".into());
        }
        if Some(claims.act) != state.activation {
            return Err("it belongs to another activation".into());
        }
        Ok(claims)
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(crate) fn refusal_outcome(refusal: Refusal) -> Outcome {
    match refusal {
        Refusal::Revoked => Outcome::Revoked,
        Refusal::Expired { at } => Outcome::Expired { at },
        Refusal::ProductUnavailable => Outcome::ProductUnavailable,
    }
}

pub(crate) fn signing_key(state: &State) -> Option<SigningKey> {
    let bytes = encoding::decode_array::<32>(state.device_key.as_deref()?)?;
    Some(SigningKey::from_bytes(&bytes))
}

/// The time by the server's clock, from the system clock and the difference last measured.
pub(crate) fn server_now(state: &State, now: DateTime<Utc>) -> DateTime<Utc> {
    // a damaged offset (too large for a date) counts as none
    TimeDelta::try_seconds(state.clock_offset_seconds).and_then(|offset| now.checked_add_signed(offset)).unwrap_or(now)
}

fn device_public_key(state: &State) -> Option<[u8; 32]> {
    signing_key(state).map(|key| key.verifying_key().to_bytes())
}
