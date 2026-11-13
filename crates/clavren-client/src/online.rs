// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:11.244813
//+ Revised: 	2026/11/11 16:32:49.778338
// -----------------------------------------------------------------------------

//! Going online: activating a key, and the check that renews the token. The state lock is never
//! held while waiting for the network, so [`Client::status`] answers at once from the UI thread.

use std::time::Duration;

use chrono::{DateTime, Utc};
use clavren_protocol::check::check_message;
use clavren_protocol::encoding;
use clavren_protocol::licence_key::LicenceKey;
use clavren_protocol::token;
use clavren_protocol::wire::{ActivationResponse, ChallengeResponse, CheckResponse, ErrorCode, ErrorResponse};
use ed25519_dalek::Signer;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

use zeroize::Zeroize;

use crate::client::{Client, refusal_outcome, server_now, signing_key};
use crate::outcome::{Outcome, Temporary};
use crate::state::{Application, Refusal, State};
use crate::store::StoreError;
use crate::transport::Answer;

impl Client {
    /// Activates `typed_key` on this computer. `Err` when the device key couldn't be saved (then
    /// nothing was sent: the key would be bound to a device this computer can't prove it is), or
    /// when the activation couldn't be saved after the server accepted it; retrying then is safe,
    /// because the device key was saved first and the server knows it as the same computer.
    pub fn activate(&self, typed_key: &str) -> Result<Outcome, StoreError> {
        let Some(key) = LicenceKey::parse(typed_key) else { return Ok(Outcome::InvalidKey) };
        let public_key = self.device_public_key()?;
        let body = json!({
            "product": self.config.product,
            "key": key.canonical(),
            "device_public_key": encoding::encode(&public_key),
            "device_name": self.config.device_name,
            "platform": self.config.platform,
            "app_version": self.config.app_version,
        });
        let answer = match self.transport.post("/api/v1/activations", &body) {
            Ok(answer) => answer,
            Err(error) => {
                log::warn!("activation couldn't reach the licence server: {error}");
                return Ok(Outcome::Unreachable);
            }
        };
        if matches!(answer.status, 200 | 201) {
            let Some(ActivationResponse { activation, token, .. }) = parsed(&answer) else {
                // e.g. a proxy's page with status 200: nothing the client can keep
                log::error!("activation got HTTP {} without an activation and a token", answer.status);
                return Ok(Outcome::Unreachable);
            };
            let now = self.clock.now();
            let mut state = self.lock();
            let mut activated = state.clone();
            activated.activation = Some(activation);
            activated.refusal = None;
            activated.token = None;
            // a key in use ends any licence application sent from here
            activated.application = None;
            if let Err(problem) = self.accept_token(&mut activated, token, now) {
                // kept as activated: the next check asks again, with backoff
                log::error!("the activation's token can't be used: {problem}");
                activated.last_check = Some(now);
            }
            self.save(&activated)?;
            *state = activated;
            drop(state);
            log::info!("licence activated");
            return Ok(self.status());
        }
        Ok(match error_code(&answer) {
            Some(ErrorCode::InvalidKey) => Outcome::InvalidKey,
            Some(ErrorCode::AlreadyActivated) => Outcome::AlreadyActivatedElsewhere,
            Some(ErrorCode::LicenceRevoked) => Outcome::Revoked,
            Some(ErrorCode::LicenceExpired) => Outcome::Expired { at: None },
            Some(ErrorCode::ProductUnavailable) => Outcome::ProductUnavailable,
            Some(ErrorCode::RateLimited) => Outcome::RateLimited { retry_after: retry_after(&answer) },
            other => {
                log::error!("activation failed: HTTP {} ({other:?})", answer.status);
                Outcome::Unreachable
            }
        })
    }

    /// Asks the server whether the licence still holds and renews the token. A definitive answer
    /// (revoked, reset, expired, withdrawn) is kept and returned; anything else is `Err` and
    /// changes nothing. Without an activation but with a licence application waiting, asks how
    /// the application went instead ([`Self::check_application`]).
    pub fn check(&self) -> Result<Outcome, Temporary> {
        let (activation, key) = {
            let state = self.lock();
            match (state.activation, signing_key(&state)) {
                (Some(activation), Some(key)) => (activation, key),
                _ if matches!(state.application, Some(Application::Pending { .. })) => {
                    drop(state);
                    return self.check_application();
                }
                _ => return Ok(Outcome::NeedsActivation),
            }
        };
        let challenge = self.post(&format!("/api/v1/activations/{activation}/challenges"), &json!({}))?;
        if challenge.status != 200 {
            return self.refused(activation, &challenge);
        }
        let Some((ChallengeResponse { challenge: challenge_id, nonce: nonce_text, .. }, nonce)) = parsed(&challenge)
            .and_then(|answer: ChallengeResponse| {
                let nonce = encoding::decode_array::<32>(&answer.nonce)?;
                Some((answer, nonce))
            })
        else {
            return Err(Temporary::Server("the challenge is malformed".into()));
        };
        let signature = key.sign(check_message(activation, challenge_id, &nonce).as_bytes());
        let body = json!({
            "challenge": challenge_id,
            "nonce": nonce_text,
            "signature": encoding::encode(&signature.to_bytes()),
            "app_version": self.config.app_version,
        });
        let answer = self.post(&format!("/api/v1/activations/{activation}/checks"), &body)?;
        if answer.status != 200 {
            return self.refused(activation, &answer);
        }
        let Some(CheckResponse { token, .. }) = parsed(&answer) else {
            return Err(Temporary::Server(format!("HTTP {} without a token", answer.status)));
        };
        let now = self.clock.now();
        let mut state = self.lock();
        if state.activation != Some(activation) {
            return Err(Temporary::Server("the activation changed during the check".into()));
        }
        let mut renewed = state.clone();
        renewed.refusal = None;
        if let Err(problem) = self.accept_token(&mut renewed, token, now) {
            return Err(Temporary::Server(format!("the renewed token can't be used: {problem}")));
        }
        self.save(&renewed)?;
        *state = renewed;
        drop(state);
        log::info!("licence checked");
        Ok(self.status())
    }

    fn post(&self, path: &str, body: &Value) -> Result<Answer, Temporary> {
        self.transport.post(path, body).map_err(Temporary::Unreachable)
    }

    /// Keeps a definitive refusal; reports anything else as temporary.
    fn refused(&self, activation: Uuid, answer: &Answer) -> Result<Outcome, Temporary> {
        let refusal = match error_code(answer) {
            Some(ErrorCode::LicenceRevoked | ErrorCode::ActivationRevoked) => Refusal::Revoked,
            Some(ErrorCode::LicenceExpired) => Refusal::Expired { at: self.licence_expiry() },
            Some(ErrorCode::ProductUnavailable) => Refusal::ProductUnavailable,
            Some(ErrorCode::RateLimited) => return Err(Temporary::RateLimited(retry_after(answer))),
            other => return Err(Temporary::Server(format!("HTTP {} ({other:?})", answer.status))),
        };
        let mut state = self.lock();
        if state.activation != Some(activation) {
            // a new activation happened meanwhile; this answer is about the old one
            return Err(Temporary::Server("the activation changed during the check".into()));
        }
        // kept only once saved: a refusal that couldn't be saved changes nothing, like any failure
        let mut refused = state.clone();
        refused.token = None;
        refused.refusal = Some(refusal);
        refused.last_check = Some(self.clock.now());
        if refusal == Refusal::Revoked {
            // this activation is over for good; a new key (or the same one, after a reset) starts afresh
            refused.activation = None;
        }
        self.save(&refused)?;
        *state = refused;
        drop(state);
        log::warn!("the licence server refused the licence: {refusal:?}");
        Ok(refusal_outcome(refusal))
    }

    /// The device key's public half, creating and saving the key first if there is none.
    fn device_public_key(&self) -> Result<[u8; 32], StoreError> {
        let mut state = self.lock();
        if let Some(key) = signing_key(&state) {
            return Ok(key.verifying_key().to_bytes());
        }
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).map_err(|error| StoreError::Protect(std::io::Error::other(error.to_string())))?;
        let key = ed25519_dalek::SigningKey::from_bytes(&seed);
        state.device_key = Some(encoding::encode(&seed));
        seed.zeroize();
        self.save(&state)?;
        Ok(key.verifying_key().to_bytes())
    }

    /// The licence expiry the last token carried, for telling the user when it ended.
    fn licence_expiry(&self) -> Option<DateTime<Utc>> {
        let state = self.lock();
        let claims = token::unverified_claims(state.token.as_deref()?)?;
        DateTime::from_timestamp(claims.lic_exp?, 0)
    }
}

impl Client {
    /// Takes `token` into `state` when it can be run on, with the clock measured anew (see
    /// [`accept_clock`]); leaves `state` as it was when it can't.
    fn accept_token(&self, state: &mut State, token: String, now: DateTime<Utc>) -> Result<(), String> {
        let issued_at = token::unverified_claims(&token).ok_or("it is not a token")?.iat;
        let mut accepted = state.clone();
        accept_clock(&mut accepted, now, issued_at);
        self.usable_claims(&accepted, &token, server_now(&accepted, now))?;
        accepted.token = Some(token);
        *state = accepted;
        Ok(())
    }
}

/// After the server vouched for the licence, the system clock as it is now is the baseline again,
/// and its distance from the server's clock is measured anew: a computer whose clock is simply
/// wrong works normally, and a clock set back before a check gains nothing, because tokens are
/// judged by the server's time. The server's time is the token's signed issue time (`iat`), never
/// the answer's unsigned `server_time`, which anything between the two could change.
fn accept_clock(state: &mut State, now: DateTime<Utc>, issued_at: i64) {
    state.last_check = Some(now);
    state.latest_seen = Some(now);
    state.last_seen = Some(now);
    state.rewound_seconds = 0;
    state.clock_offset_seconds = issued_at.saturating_sub(now.timestamp());
}

/// The answer's body as `T`, the shape both sides share; `None` when it isn't one.
pub(crate) fn parsed<T: DeserializeOwned>(answer: &Answer) -> Option<T> {
    T::deserialize(&answer.body).ok()
}

pub(crate) fn error_code(answer: &Answer) -> Option<ErrorCode> {
    parsed::<ErrorResponse>(answer).and_then(|error| ErrorCode::parse(&error.error))
}

pub(crate) fn retry_after(answer: &Answer) -> Duration {
    Duration::from_secs(answer.retry_after.unwrap_or(60))
}
