// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.407002
//+ Revised: 	2026/11/10 17:59:19.531054
// -----------------------------------------------------------------------------

//! Activation tokens: JWTs signed with Ed25519 (`alg` `EdDSA`), which applications check offline
//! against the public keys they embed. Only this one shape is produced or accepted: three base64url
//! parts, header `{"alg":"EdDSA","typ":"JWT","kid":…}`, and the claims below.

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::encoding;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// The public address of the Clavren that issued it.
    pub iss: String,
    /// The product code.
    pub aud: String,
    /// The licence ID.
    pub sub: Uuid,
    /// The activation ID.
    pub act: Uuid,
    /// [`crate::check::device_hash`] of the device's public key.
    pub dev: String,
    /// When the licence itself expires (Unix seconds); absent for a permanent licence.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub lic_exp: Option<i64>,
    /// A test licence (for trying the application, not a sale); absent for any other.
    #[serde(skip_serializing_if = "is_false", default)]
    pub test: bool,
    pub iat: i64,
    /// When the token expires: 30 days after issuing, or the licence expiry if sooner.
    pub exp: i64,
    pub jti: Uuid,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Serialize, Deserialize)]
struct Header {
    alg: String,
    typ: String,
    kid: String,
}

/// The public keys an application trusts, by key ID (`v1`, `v2`…).
pub type TrustedKeys = BTreeMap<String, VerifyingKey>;

pub fn sign(claims: &Claims, kid: &str, key: &SigningKey) -> String {
    let header = Header { alg: "EdDSA".into(), typ: "JWT".into(), kid: kid.into() };
    // serialising plain structs can't fail
    let header = encoding::encode(&serde_json::to_vec(&header).unwrap_or_default());
    let claims = encoding::encode(&serde_json::to_vec(claims).unwrap_or_default());
    let signing_input = format!("{header}.{claims}");
    let signature = encoding::encode(&key.sign(signing_input.as_bytes()).to_bytes());
    format!("{signing_input}.{signature}")
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    #[error("the token is malformed")]
    Malformed,
    #[error("the token was signed with a key this application doesn't trust")]
    UnknownKey,
    #[error("the token's signature is wrong")]
    BadSignature,
    #[error("the token is for another product")]
    WrongProduct,
    #[error("the token has expired")]
    Expired,
}

/// The claims of `token` if its signature checks out with one of `trusted`, it is for `product`,
/// and it hasn't expired at `now` (Unix seconds).
pub fn verify(token: &str, trusted: &TrustedKeys, product: &str, now: i64) -> Result<Claims, TokenError> {
    let mut parts = token.split('.');
    let (Some(header), Some(claims), Some(signature), None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(TokenError::Malformed);
    };
    let parsed: Header = parse(header)?;
    if parsed.alg != "EdDSA" || parsed.typ != "JWT" {
        return Err(TokenError::Malformed);
    }
    let key = trusted.get(&parsed.kid).ok_or(TokenError::UnknownKey)?;
    let signature = encoding::decode_array::<64>(signature).ok_or(TokenError::Malformed)?;
    let signing_input = &token[..header.len() + 1 + claims.len()];
    key.verify_strict(signing_input.as_bytes(), &Signature::from_bytes(&signature))
        .map_err(|_| TokenError::BadSignature)?;
    let claims: Claims = parse(claims)?;
    if claims.aud != product {
        return Err(TokenError::WrongProduct);
    }
    if claims.exp <= now {
        return Err(TokenError::Expired);
    }
    Ok(claims)
}

/// The claims of `token` without checking anything: only for showing what a token said (its
/// licence expiry, say), never for deciding whether it is valid.
pub fn unverified_claims(token: &str) -> Option<Claims> {
    parse(token.split('.').nth(1)?).ok()
}

fn parse<T: for<'de> Deserialize<'de>>(part: &str) -> Result<T, TokenError> {
    encoding::decode(part).and_then(|bytes| serde_json::from_slice(&bytes).ok()).ok_or(TokenError::Malformed)
}

#[cfg(test)]
mod tests;
