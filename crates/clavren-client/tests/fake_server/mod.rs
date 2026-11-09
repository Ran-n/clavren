// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:11.926414
//+ Revised: 	2026/11/08 14:55:43.724241
// -----------------------------------------------------------------------------

//! A licence server in memory: answers the public API as Clavren does, with tokens signed by a
//! test key, and can be told to refuse, fail or go offline.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use clavren_client::clock::{Clock, ManualClock};
use clavren_client::transport::{Answer, Transport};
use clavren_protocol::check::{check_message, device_hash};
use clavren_protocol::encoding;
use clavren_protocol::token::{self, Claims, TrustedKeys};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use serde_json::{Value, json};
use uuid::Uuid;

pub const PRODUCT: &str = "avalio-lite";

/// What the server does with the next requests.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    /// Answers every request with this error code and status.
    Refuse(&'static str, u16),
    /// No answer at all (no network).
    Offline,
    /// Status 200 with a body that isn't Clavren's (a proxy's or captive portal's page).
    Garbage,
    /// Tokens signed with a key the client doesn't trust (a signing key it wasn't built with).
    UntrustedToken,
}

pub struct FakeServer {
    pub clock: Arc<ManualClock>,
    pub mode: Mutex<Mode>,
    pub calls: AtomicUsize,
    pub licence_expires_at: Mutex<Option<DateTime<Utc>>>,
    signing: SigningKey,
    device: Mutex<Option<[u8; 32]>>,
    nonces: Mutex<Vec<(Uuid, [u8; 32])>>,
}

impl FakeServer {
    pub fn new(clock: Arc<ManualClock>) -> Arc<Self> {
        Arc::new(Self {
            clock,
            mode: Mutex::new(Mode::Normal),
            calls: AtomicUsize::new(0),
            licence_expires_at: Mutex::new(None),
            signing: SigningKey::from_bytes(&[42; 32]),
            device: Mutex::new(None),
            nonces: Mutex::new(Vec::new()),
        })
    }

    pub fn trusted_keys() -> TrustedKeys {
        TrustedKeys::from([("v1".to_string(), SigningKey::from_bytes(&[42; 32]).verifying_key())])
    }

    pub fn set_mode(&self, mode: Mode) {
        *self.mode.lock().unwrap() = mode;
    }

    fn token(&self, device: &[u8; 32]) -> Value {
        let now = self.clock.now();
        let licence_end = *self.licence_expires_at.lock().unwrap();
        let end = licence_end.map_or(now + chrono::Duration::days(30), |at| at.min(now + chrono::Duration::days(30)));
        let untrusted = *self.mode.lock().unwrap() == Mode::UntrustedToken;
        let claims = Claims {
            iss: "https://licences.example.com".into(),
            aud: PRODUCT.into(),
            sub: Uuid::from_u128(1),
            act: Uuid::from_u128(2),
            dev: device_hash(device),
            lic_exp: licence_end.map(|at| at.timestamp()),
            iat: now.timestamp(),
            exp: end.timestamp(),
            jti: Uuid::from_u128(3),
        };
        json!({
            "activation": Uuid::from_u128(2),
            "token": if untrusted {
                token::sign(&claims, "v1", &SigningKey::from_bytes(&[7; 32]))
            } else {
                token::sign(&claims, "v1", &self.signing)
            },
            "token_expires_at": end,
            "licence_expires_at": licence_end,
            "server_time": now,
        })
    }
}

fn answer(status: u16, body: Value) -> Answer {
    Answer { status, body, retry_after: if status == 429 { Some(120) } else { None } }
}

impl Transport for FakeServer {
    fn post(&self, path: &str, body: &Value) -> Result<Answer, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let now = self.clock.now();
        match self.mode.lock().unwrap().clone() {
            Mode::Offline => return Err("connection refused".into()),
            Mode::Refuse(code, status) => {
                return Ok(answer(status, json!({ "error": code, "message": "no", "server_time": now })));
            }
            Mode::Garbage => return Ok(answer(200, Value::Null)),
            Mode::Normal | Mode::UntrustedToken => {}
        }
        if path == "/api/v1/activations" {
            assert_eq!(body["product"], PRODUCT);
            let device = encoding::decode_array::<32>(body["device_public_key"].as_str().unwrap()).unwrap();
            *self.device.lock().unwrap() = Some(device);
            return Ok(answer(201, self.token(&device)));
        }
        if path.ends_with("/challenges") {
            let (id, nonce) = (Uuid::from_u128(1000 + self.calls.load(Ordering::SeqCst) as u128), [7u8; 32]);
            self.nonces.lock().unwrap().push((id, nonce));
            return Ok(answer(
                200,
                json!({ "challenge": id, "nonce": encoding::encode(&nonce), "expires_at": now, "server_time": now }),
            ));
        }
        // a check: the signature must come from the activated device
        let challenge = Uuid::parse_str(body["challenge"].as_str().unwrap()).unwrap();
        let nonce =
            self.nonces.lock().unwrap().iter().find(|(id, _)| *id == challenge).map(|(_, nonce)| *nonce).unwrap();
        let device = self.device.lock().unwrap().unwrap();
        let signature = encoding::decode_array::<64>(body["signature"].as_str().unwrap()).unwrap();
        let message = check_message(Uuid::from_u128(2), challenge, &nonce);
        VerifyingKey::from_bytes(&device)
            .unwrap()
            .verify_strict(message.as_bytes(), &Signature::from_bytes(&signature))
            .unwrap();
        Ok(answer(200, self.token(&device)))
    }
}
