// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.977097
//+ Revised: 	2026/11/03 04:54:19.392656
// -----------------------------------------------------------------------------

//! Encrypting small secrets at rest (licence keys, TOTP secrets) with XChaCha20-Poly1305: a fresh
//! random 24-byte nonce per value, and "additional data" binding each ciphertext to the row it
//! belongs to, so a value copied into another row fails to decrypt.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

use super::key_ring::SubKey;
use super::random::{RandomError, random_bytes};

pub const NONCE_LENGTH: usize = 24;

pub struct Sealed {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; NONCE_LENGTH],
}

#[derive(Debug, thiserror::Error)]
pub enum AeadError {
    #[error(transparent)]
    Random(#[from] RandomError),
    #[error("encryption failed")]
    Encrypt,
    /// Wrong key, wrong additional data, or a changed ciphertext: indistinguishable on purpose.
    #[error("a stored secret could not be decrypted (wrong master key, or the data was changed)")]
    Decrypt,
    #[error("a stored nonce is {0} bytes long instead of {NONCE_LENGTH}")]
    NonceLength(usize),
}

fn cipher(key: &SubKey) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(&(*key.as_bytes()).into())
}

pub fn seal(key: &SubKey, plaintext: &[u8], additional_data: &[u8]) -> Result<Sealed, AeadError> {
    let nonce = random_bytes::<NONCE_LENGTH>()?;
    let ciphertext = cipher(key)
        .encrypt(&XNonce::from(nonce), Payload { msg: plaintext, aad: additional_data })
        .map_err(|_| AeadError::Encrypt)?;
    Ok(Sealed { ciphertext, nonce })
}

pub fn open(key: &SubKey, ciphertext: &[u8], nonce: &[u8], additional_data: &[u8]) -> Result<Vec<u8>, AeadError> {
    let nonce: [u8; NONCE_LENGTH] = nonce.try_into().map_err(|_| AeadError::NonceLength(nonce.len()))?;
    cipher(key)
        .decrypt(&XNonce::from(nonce), Payload { msg: ciphertext, aad: additional_data })
        .map_err(|_| AeadError::Decrypt)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::crypto::key_ring::KeyRing;
    use crate::crypto::master_key::MasterKey;

    fn ring(byte: u8) -> KeyRing {
        KeyRing::new(MasterKey::from_bytes([byte; 32]))
    }

    #[test]
    fn sealing_twice_gives_different_nonces_and_ciphertexts() {
        let keys = ring(1);
        let (first, second) = (
            seal(keys.totp_encryption(), b"secret", b"row").unwrap(),
            seal(keys.totp_encryption(), b"secret", b"row").unwrap(),
        );
        assert_ne!(first.nonce, second.nonce);
        assert_ne!(first.ciphertext, second.ciphertext);
    }

    #[test]
    fn the_wrong_key_fails() {
        let sealed = seal(ring(1).totp_encryption(), b"secret", b"row").unwrap();
        let opened = open(ring(2).totp_encryption(), &sealed.ciphertext, &sealed.nonce, b"row");
        assert!(matches!(opened, Err(AeadError::Decrypt)));
    }

    #[test]
    fn another_purpose_key_fails() {
        let keys = ring(1);
        let sealed = seal(keys.totp_encryption(), b"secret", b"row").unwrap();
        assert!(open(keys.licence_encryption(), &sealed.ciphertext, &sealed.nonce, b"row").is_err());
    }

    #[test]
    fn other_additional_data_fails() {
        let keys = ring(1);
        let sealed = seal(keys.totp_encryption(), b"secret", b"row-1").unwrap();
        assert!(open(keys.totp_encryption(), &sealed.ciphertext, &sealed.nonce, b"row-2").is_err());
    }

    #[test]
    fn a_tampered_ciphertext_fails() {
        let keys = ring(1);
        let mut sealed = seal(keys.totp_encryption(), b"secret", b"row").unwrap();
        sealed.ciphertext[0] ^= 1;
        assert!(open(keys.totp_encryption(), &sealed.ciphertext, &sealed.nonce, b"row").is_err());
    }

    #[test]
    fn a_nonce_of_the_wrong_length_is_reported() {
        let keys = ring(1);
        let sealed = seal(keys.totp_encryption(), b"secret", b"row").unwrap();
        let opened = open(keys.totp_encryption(), &sealed.ciphertext, &sealed.nonce[..12], b"row");
        assert!(matches!(opened, Err(AeadError::NonceLength(12))));
    }

    proptest! {
        #[test]
        fn opening_returns_what_was_sealed(plaintext in proptest::collection::vec(any::<u8>(), 0..256), aad in proptest::collection::vec(any::<u8>(), 0..64)) {
            let keys = ring(7);
            let sealed = seal(keys.licence_encryption(), &plaintext, &aad).unwrap();
            prop_assert_eq!(open(keys.licence_encryption(), &sealed.ciphertext, &sealed.nonce, &aad).unwrap(), plaintext);
        }
    }
}
