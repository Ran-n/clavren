// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.425468
//+ Revised: 	2026/11/03 04:54:16.425468
// -----------------------------------------------------------------------------

//! A computer running a licensed application, as the public API sees it: a device key pair, an
//! activation, challenges signed with the device key, and token checks.

use axum::Router;
use axum::http::Method;
use clavren::activations::tokens;
use clavren::crypto::key_ring::KeyRing;
use clavren::crypto::master_key::MasterKey;
use clavren_protocol::check::check_message;
use clavren_protocol::encoding;
use clavren_protocol::token::{self, Claims, TrustedKeys};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Value, json};
use uuid::Uuid;

use super::TEST_MASTER_KEY;
use super::api::{Call, Reply};

pub struct Device {
    key: SigningKey,
    pub name: String,
}

impl Device {
    pub fn new(seed: u8) -> Self {
        Self { key: SigningKey::from_bytes(&[seed; 32]), name: format!("PC {seed}") }
    }

    pub fn public_key(&self) -> String {
        encoding::encode(self.key.verifying_key().as_bytes())
    }

    pub fn activation_body(&self, product: &str, key: &str) -> Value {
        json!({
            "product": product,
            "key": key,
            "device_public_key": self.public_key(),
            "device_name": self.name,
            "platform": "windows-x86_64",
            "app_version": "1.0.0",
        })
    }

    pub async fn activate(&self, app: &Router, product: &str, key: &str) -> Reply {
        Call::new(Method::POST, "/api/v1/activations").json(self.activation_body(product, key)).send(app).await
    }

    pub async fn challenge(&self, app: &Router, activation: &str) -> Reply {
        Call::new(Method::POST, &format!("/api/v1/activations/{activation}/challenges")).send(app).await
    }

    /// The body of a check answering `challenge` (a challenge reply's body), signed by this device.
    pub fn check_body(&self, activation: &str, challenge: &Value) -> Value {
        let challenge_id = challenge["challenge"].as_str().unwrap();
        let nonce = encoding::decode_array::<32>(challenge["nonce"].as_str().unwrap()).unwrap();
        let message =
            check_message(Uuid::parse_str(activation).unwrap(), Uuid::parse_str(challenge_id).unwrap(), &nonce);
        json!({
            "challenge": challenge_id,
            "nonce": challenge["nonce"],
            "signature": encoding::encode(&self.key.sign(message.as_bytes()).to_bytes()),
            "app_version": "1.0.1",
        })
    }

    pub async fn send_check(&self, app: &Router, activation: &str, body: Value) -> Reply {
        Call::new(Method::POST, &format!("/api/v1/activations/{activation}/checks")).json(body).send(app).await
    }

    /// A challenge, then a check answering it.
    pub async fn check(&self, app: &Router, activation: &str) -> Reply {
        let challenge = self.challenge(app, activation).await;
        assert_eq!(challenge.status, axum::http::StatusCode::OK, "{:?}", challenge.body);
        self.send_check(app, activation, self.check_body(activation, &challenge.body)).await
    }
}

/// The keys an application built against the test server would embed.
pub fn trusted_keys() -> TrustedKeys {
    let keys = KeyRing::new(MasterKey::from_bytes(TEST_MASTER_KEY));
    TrustedKeys::from([(tokens::kid(1), tokens::public_key(&keys, 1))])
}

/// The claims of a token from the test server, checked as an application would.
pub fn verified(reply: &Reply, product: &str, now: i64) -> Claims {
    token::verify(reply.body["token"].as_str().unwrap(), &trusted_keys(), product, now).unwrap()
}
