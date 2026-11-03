// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.191036
//+ Revised: 	2026/11/03 04:54:12.191036
// -----------------------------------------------------------------------------

//! What an application signs to prove it holds its device key: the challenge the server just gave
//! it, bound to its activation. The device's clock plays no part, so a PC with the wrong time can
//! still check in.

use uuid::Uuid;

use crate::encoding;

pub const CHECK_MESSAGE_VERSION: &str = "clavren-check-v1";
pub const NONCE_LENGTH: usize = 32;

/// `clavren-check-v1`, the activation ID, the challenge ID and the nonce (base64url), one per
/// line, joined with `\n` and without a trailing newline. The device signs its UTF-8 bytes.
pub fn check_message(activation: Uuid, challenge: Uuid, nonce: &[u8; NONCE_LENGTH]) -> String {
    format!("{CHECK_MESSAGE_VERSION}\n{activation}\n{challenge}\n{}", encoding::encode(nonce))
}

/// What the `dev` claim of a token holds: base64url of the BLAKE3 hash of the device's public key.
pub fn device_hash(device_public_key: &[u8; 32]) -> String {
    encoding::encode(blake3::hash(device_public_key).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Published in `docs/api.md`.
    #[test]
    fn test_vectors() {
        let activation = Uuid::parse_str("01900000-0000-7000-8000-000000000001").unwrap();
        let challenge = Uuid::parse_str("01900000-0000-7000-8000-000000000002").unwrap();
        let nonce: [u8; NONCE_LENGTH] = std::array::from_fn(|index| index as u8);
        assert_eq!(
            check_message(activation, challenge, &nonce),
            "clavren-check-v1\n01900000-0000-7000-8000-000000000001\n01900000-0000-7000-8000-000000000002\nAAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"
        );
        // BLAKE3 of 32 zero bytes is 2ada83c1…20fcda (b3sum)
        assert_eq!(device_hash(&[0; 32]), "KtqDwYGaU3La4SOPwd7RI8gQT9qhWGKq7mlCihgg_No");
    }
}
