// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:11.617928
//+ Revised: 	2026/11/03 04:54:11.617928
// -----------------------------------------------------------------------------

//! What the client keeps between runs: the device key, the activation, the cached token, the
//! latest time it has seen (to notice a clock set back), when it last checked, and a definitive
//! refusal it was given. All of it is stored protected (see [`crate::store`]).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

#[derive(Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// The Ed25519 private key, base64url.
    pub device_key: Option<String>,
    pub activation: Option<Uuid>,
    pub token: Option<String>,
    /// The latest of the system clock, token issue times and server times seen.
    pub latest_seen: Option<DateTime<Utc>>,
    pub last_check: Option<DateTime<Utc>>,
    /// A definitive answer from the server, kept until a new activation succeeds.
    pub refusal: Option<Refusal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    Revoked,
    Expired { at: Option<DateTime<Utc>> },
    ProductUnavailable,
}

impl State {
    pub fn saw(&mut self, time: DateTime<Utc>) {
        self.latest_seen = Some(self.latest_seen.map_or(time, |latest| latest.max(time)));
    }
}

/// Never prints the key or the token.
impl std::fmt::Debug for State {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("State")
            .field("device_key", &self.device_key.as_ref().map(|_| "<redacted>"))
            .field("activation", &self.activation)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("latest_seen", &self.latest_seen)
            .field("last_check", &self.last_check)
            .field("refusal", &self.refusal)
            .finish()
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.device_key.zeroize();
        self.token.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_time_seen_only_moves_forward() {
        let mut state = State::default();
        let (early, late) = (DateTime::from_timestamp(100, 0).unwrap(), DateTime::from_timestamp(200, 0).unwrap());
        state.saw(late);
        state.saw(early);
        assert_eq!(state.latest_seen, Some(late));
    }

    #[test]
    fn debug_output_hides_secrets() {
        let state =
            State { device_key: Some("secret-key".into()), token: Some("secret-token".into()), ..State::default() };
        let printed = format!("{state:?}");
        assert!(!printed.contains("secret-key") && !printed.contains("secret-token"), "{printed}");
    }
}
