// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:10.584826
//+ Revised: 	2026/11/03 04:54:10.584826
// -----------------------------------------------------------------------------

//! The licence of one application on this computer: its offline status, and when to check online.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use clavren_protocol::check::device_hash;
use clavren_protocol::encoding;
use clavren_protocol::token::{self, TrustedKeys};
use ed25519_dalek::SigningKey;

use crate::clock::Clock;
use crate::outcome::Outcome;
use crate::state::{Refusal, State};
use crate::store::{SecretStore, StoreError};
use crate::transport::Transport;

/// A clock more than this far behind the latest time seen was set back on purpose.
pub const ROLLBACK_TOLERANCE: TimeDelta = TimeDelta::hours(1);

pub struct Config {
    /// The product code, as in Clavren.
    pub product: String,
    /// The token-signing public keys this build trusts (`clavren-admin show-public-key`).
    pub trusted_keys: TrustedKeys,
    pub app_version: String,
    /// Shown to the administrator, e.g. the computer's name.
    pub device_name: String,
    pub platform: String,
    /// Check online once this long has passed since the last successful check.
    pub check_interval: Duration,
}

impl Config {
    /// Checks daily; names the device after the computer.
    pub fn new(product: &str, trusted_keys: TrustedKeys, app_version: &str) -> Self {
        Self {
            product: product.to_string(),
            trusted_keys,
            app_version: app_version.to_string(),
            device_name: std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| "computer".into()),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            check_interval: Duration::from_secs(24 * 3600),
        }
    }
}

pub struct Client {
    pub(crate) config: Config,
    pub(crate) store: Box<dyn SecretStore>,
    pub(crate) transport: Box<dyn Transport>,
    pub(crate) clock: Box<dyn Clock>,
    pub(crate) state: Mutex<State>,
}

impl Client {
    /// Loads the saved state. Unreadable state (another Windows account, a damaged file) counts
    /// as none: the application asks for its key again.
    pub fn new(
        config: Config,
        store: Box<dyn SecretStore>,
        transport: Box<dyn Transport>,
        clock: Box<dyn Clock>,
    ) -> Self {
        let state = match store.load() {
            Ok(Some(bytes)) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
                log::warn!("the saved licence state is damaged ({error}); starting over");
                State::default()
            }),
            Ok(None) => State::default(),
            Err(error) => {
                log::warn!("the saved licence state can't be read ({error}); starting over");
                State::default()
            }
        };
        Self { config, store, transport, clock, state: Mutex::new(state) }
    }

    /// The licence as far as this computer knows, without going online.
    pub fn status(&self) -> Outcome {
        let now = self.clock.now();
        let mut state = self.lock();
        let outcome = self.offline_outcome(&state, now);
        if outcome.is_licensed() && state.latest_seen.is_none_or(|latest| now > latest) {
            state.saw(now);
            if let Err(error) = self.save(&state) {
                log::error!("couldn't record the time seen: {error}");
            }
        }
        outcome
    }

    fn offline_outcome(&self, state: &State, now: DateTime<Utc>) -> Outcome {
        if let Some(refusal) = state.refusal {
            return refusal_outcome(refusal);
        }
        let Some(token) = state.token.as_deref() else {
            return if state.activation.is_some() { Outcome::NeedsCheck } else { Outcome::NeedsActivation };
        };
        if state.latest_seen.is_some_and(|latest| now < latest - ROLLBACK_TOLERANCE) {
            log::warn!("the clock is behind the latest time seen; the licence must be checked online");
            return Outcome::NeedsCheck;
        }
        let claims = match token::verify(token, &self.config.trusted_keys, &self.config.product, now.timestamp()) {
            Ok(claims) => claims,
            Err(error) => {
                log::info!("the cached licence token can't be used: {error}");
                return Outcome::NeedsCheck;
            }
        };
        if Some(claims.dev) != device_public_key(state).map(|key| device_hash(&key)) {
            log::warn!("the cached licence token belongs to another device");
            return Outcome::NeedsCheck;
        }
        Outcome::Licensed {
            licence_expires_at: claims.lic_exp.and_then(|at| DateTime::from_timestamp(at, 0)),
            offline_until: DateTime::from_timestamp(claims.exp, 0).unwrap_or(now),
        }
    }

    /// Whether an online check is due: the last successful one is older than the interval, or
    /// there is nothing valid to run on. A revoked licence is never checked again.
    pub fn needs_check(&self) -> bool {
        self.time_until_due().is_zero()
    }

    /// How long until the next online check is due (zero when it is due now).
    pub fn time_until_due(&self) -> Duration {
        let now = self.clock.now();
        let state = self.lock();
        if state.activation.is_none() {
            return Duration::MAX;
        }
        let licensed = self.offline_outcome(&state, now).is_licensed();
        match state.last_check {
            // a refusal is an answer too: ask again at the usual pace (an expiry may be extended)
            Some(last) if licensed || state.refusal.is_some() => {
                let due = last + TimeDelta::from_std(self.config.check_interval).unwrap_or(TimeDelta::MAX);
                (due - now).to_std().unwrap_or(Duration::ZERO)
            }
            _ => Duration::ZERO,
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn save(&self, state: &State) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec(state).map_err(|_| StoreError::Corrupt)?;
        self.store.save(&bytes)
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

fn device_public_key(state: &State) -> Option<[u8; 32]> {
    signing_key(state).map(|key| key.verifying_key().to_bytes())
}
