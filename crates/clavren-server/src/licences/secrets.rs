// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.750504
//+ Revised: 	2026/11/03 04:54:15.750504
// -----------------------------------------------------------------------------

//! A licence key's two stored forms: the lookup hash (keyed BLAKE3 of the canonical key), which
//! finds the licence when an application sends the key, and the encrypted key
//! (XChaCha20-Poly1305), which lets the administrator show it again. The licence and product IDs
//! are bound in as additional data, so a ciphertext copied onto another licence won't open.

use clavren_protocol::licence_key::{LicenceKey, RANDOM_BYTES};
use uuid::Uuid;

use crate::crypto::aead::{self, AeadError, Sealed};
use crate::crypto::key_ring::KeyRing;
use crate::crypto::random::{RandomError, random_bytes};

/// Bumped if keys are ever encrypted differently; stored with each ciphertext.
pub const ENCRYPTION_VERSION: i16 = 1;

pub fn generate() -> Result<LicenceKey, RandomError> {
    Ok(LicenceKey::from_random(random_bytes::<RANDOM_BYTES>()?))
}

pub fn lookup_hash(keys: &KeyRing, key: &LicenceKey) -> [u8; 32] {
    *blake3::keyed_hash(keys.licence_lookup().as_bytes(), key.canonical().as_bytes()).as_bytes()
}

pub fn seal(keys: &KeyRing, key: &LicenceKey, licence_id: Uuid, product_id: Uuid) -> Result<Sealed, AeadError> {
    aead::seal(keys.licence_encryption(), key.canonical().as_bytes(), &additional_data(licence_id, product_id))
}

pub fn open(
    keys: &KeyRing,
    ciphertext: &[u8],
    nonce: &[u8],
    licence_id: Uuid,
    product_id: Uuid,
) -> Result<LicenceKey, SecretError> {
    let plaintext = aead::open(keys.licence_encryption(), ciphertext, nonce, &additional_data(licence_id, product_id))?;
    std::str::from_utf8(&plaintext).ok().and_then(LicenceKey::parse).ok_or(SecretError::NotAKey)
}

fn additional_data(licence_id: Uuid, product_id: Uuid) -> [u8; 32] {
    let mut data = [0; 32];
    data[..16].copy_from_slice(licence_id.as_bytes());
    data[16..].copy_from_slice(product_id.as_bytes());
    data
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error(transparent)]
    Aead(#[from] AeadError),
    #[error("a stored licence key decrypted to something that isn't a key")]
    NotAKey,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::master_key::MasterKey;

    fn keys(byte: u8) -> KeyRing {
        KeyRing::new(MasterKey::from_bytes([byte; 32]))
    }

    #[test]
    fn a_sealed_key_opens_for_its_own_licence_only() {
        let (keys, key) = (keys(1), generate().unwrap());
        let (licence, product) = (Uuid::now_v7(), Uuid::now_v7());
        let sealed = seal(&keys, &key, licence, product).unwrap();
        assert_eq!(open(&keys, &sealed.ciphertext, &sealed.nonce, licence, product).unwrap(), key);
        assert!(open(&keys, &sealed.ciphertext, &sealed.nonce, Uuid::now_v7(), product).is_err(), "another licence");
        assert!(open(&keys, &sealed.ciphertext, &sealed.nonce, licence, Uuid::now_v7()).is_err(), "another product");
        assert!(
            open(&self::keys(2), &sealed.ciphertext, &sealed.nonce, licence, product).is_err(),
            "another master key"
        );
        let mut tampered = sealed.ciphertext.clone();
        tampered[0] ^= 1;
        assert!(open(&keys, &tampered, &sealed.nonce, licence, product).is_err(), "a changed ciphertext");
    }

    #[test]
    fn lookup_hashes_depend_on_the_key_and_the_master_key() {
        let key = LicenceKey::parse("BRN-79YN-44XQ-W025-377Z").unwrap();
        assert_eq!(lookup_hash(&keys(1), &key), lookup_hash(&keys(1), &key));
        assert_ne!(lookup_hash(&keys(1), &key), lookup_hash(&keys(2), &key));
        assert_ne!(lookup_hash(&keys(1), &key), lookup_hash(&keys(1), &generate().unwrap()));
        assert_ne!(
            lookup_hash(&keys(1), &key),
            *blake3::hash(key.canonical().as_bytes()).as_bytes(),
            "keyed, not plain"
        );
    }

    #[test]
    fn something_other_than_a_key_is_refused_after_decrypting() {
        let keys = keys(1);
        let (licence, product) = (Uuid::now_v7(), Uuid::now_v7());
        let sealed = aead::seal(keys.licence_encryption(), b"not a key", &additional_data(licence, product)).unwrap();
        assert!(matches!(open(&keys, &sealed.ciphertext, &sealed.nonce, licence, product), Err(SecretError::NotAKey)));
    }
}
