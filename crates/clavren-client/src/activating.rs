// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:48:57.873927
//+ Revised: 	2026/11/14 11:48:57.873927
// -----------------------------------------------------------------------------

//! Activating a key on this computer, with the device key made and saved first.

use crate::client::{Client, signing_key};
use crate::online::{device_name, error_code, parsed, retry_after};
use crate::outcome::Outcome;
use crate::store::StoreError;
use clavren_protocol::encoding;
use clavren_protocol::licence_key::LicenceKey;
use clavren_protocol::wire::{ActivationResponse, ErrorCode};
use serde_json::json;
use zeroize::Zeroize;

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
            "device_name": device_name(&self.config.device_name),
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
            self.commit(&mut state, activated)?;
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

    /// The device key's public half, creating and saving the key first if there is none.
    fn device_public_key(&self) -> Result<[u8; 32], StoreError> {
        let mut state = self.lock();
        if let Some(key) = signing_key(&state) {
            return Ok(key.verifying_key().to_bytes());
        }
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).map_err(|error| StoreError::Protect(std::io::Error::other(error.to_string())))?;
        let key = ed25519_dalek::SigningKey::from_bytes(&seed);
        // kept only once saved: an unsaved key must never reach the server, where the licence
        // would be bound to a device this computer can't prove it is after a restart
        let mut with_key = state.clone();
        with_key.device_key = Some(encoding::encode(&seed));
        seed.zeroize();
        self.commit(&mut state, with_key)?;
        Ok(key.verifying_key().to_bytes())
    }
}
