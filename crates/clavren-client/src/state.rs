// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:11.617928
//+ Revised: 	2026/11/11 16:32:49.948771
// -----------------------------------------------------------------------------

//! What the client keeps between runs: the device key, the activation, the cached token, the
//! latest and the previous time the system clock showed and how far it was set back in all since
//! the last check (to notice a clock set back, at once or a little at a time), how far it was
//! from the server's at the last answer, when it last checked, a definitive refusal it was
//! given, and a licence application it sent (its secret while it waits, the reason once
//! rejected). All of it is stored protected (see [`crate::store`]).

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
    /// The latest time the system clock showed (system-clock time, so a wrong clock still works).
    pub latest_seen: Option<DateTime<Utc>>,
    /// The previous time the system clock showed, to add up how far it is set back.
    #[serde(default)]
    pub last_seen: Option<DateTime<Utc>>,
    /// How far the system clock was set back in all since the last successful check, in seconds:
    /// many small steps back count like one big one.
    #[serde(default)]
    pub rewound_seconds: i64,
    /// Server time minus system-clock time at the last successful answer, in seconds. Tokens are
    /// judged by the server's time (the system clock plus this), so a clock set back before a
    /// check doesn't stretch the token past its 30 days.
    #[serde(default)]
    pub clock_offset_seconds: i64,
    pub last_check: Option<DateTime<Utc>>,
    /// A definitive answer from the server, kept until a new activation succeeds.
    pub refusal: Option<Refusal>,
    /// A licence application sent from this computer, until its key is activated or the
    /// application set aside.
    #[serde(default)]
    pub application: Option<Application>,
}

/// A licence application this computer sent.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum Application {
    /// Waiting for the administrator: the secret asks how it went.
    Pending { id: Uuid, secret: String, last_check: Option<DateTime<Utc>> },
    /// Turned down (or withdrawn), with the administrator's reason when there is one.
    Rejected { reason: Option<String> },
}

/// Never prints the secret.
impl std::fmt::Debug for Application {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending { id, last_check, .. } => formatter
                .debug_struct("Pending")
                .field("id", id)
                .field("secret", &"<redacted>")
                .field("last_check", last_check)
                .finish(),
            Self::Rejected { reason } => formatter.debug_struct("Rejected").field("reason", reason).finish(),
        }
    }
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
        self.rewound_seconds = self.rewound_seconds.saturating_add(self.step_back(time));
        self.last_seen = Some(time);
        self.latest_seen = Some(self.latest_seen.map_or(time, |latest| latest.max(time)));
    }

    /// How far the clock was set back in all, counting `now`.
    pub fn rewound_by(&self, now: DateTime<Utc>) -> i64 {
        self.rewound_seconds.saturating_add(self.step_back(now))
    }

    /// The seconds `time` is behind the previous time seen (0 when it isn't).
    fn step_back(&self, time: DateTime<Utc>) -> i64 {
        self.last_seen.map_or(0, |last| last.timestamp().saturating_sub(time.timestamp()).max(0))
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
            .field("last_seen", &self.last_seen)
            .field("rewound_seconds", &self.rewound_seconds)
            .field("clock_offset_seconds", &self.clock_offset_seconds)
            .field("last_check", &self.last_check)
            .field("refusal", &self.refusal)
            .field("application", &self.application)
            .finish()
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.device_key.zeroize();
        self.token.zeroize();
        if let Some(Application::Pending { secret, .. }) = &mut self.application {
            secret.zeroize();
        }
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
    fn small_steps_back_add_up() {
        let mut state = State::default();
        let at = |seconds| DateTime::from_timestamp(seconds, 0).unwrap();
        state.saw(at(10_000));
        state.saw(at(9_000));
        state.saw(at(9_500));
        state.saw(at(8_800));
        assert_eq!(state.rewound_seconds, 1_000 + 700);
        assert_eq!(state.latest_seen, Some(at(10_000)));
        assert_eq!(state.rewound_by(at(8_000)), 1_700 + 800);
        assert_eq!(state.rewound_by(at(9_000)), 1_700, "moving forward adds nothing");
    }

    #[test]
    fn state_saved_before_applications_existed_still_loads() {
        let state: State = serde_json::from_str(
            r#"{"device_key":null,"activation":null,"token":null,"latest_seen":null,"last_check":null,"refusal":null}"#,
        )
        .unwrap();
        assert_eq!(state.application, None);
    }

    #[test]
    fn debug_output_hides_secrets() {
        let application = Application::Pending { id: Uuid::nil(), secret: "secret-ask".into(), last_check: None };
        let state = State {
            device_key: Some("secret-key".into()),
            token: Some("secret-token".into()),
            application: Some(application),
            ..State::default()
        };
        let printed = format!("{state:?}");
        for secret in ["secret-key", "secret-token", "secret-ask"] {
            assert!(!printed.contains(secret), "{printed}");
        }
    }
}
