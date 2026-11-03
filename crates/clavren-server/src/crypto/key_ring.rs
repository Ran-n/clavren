// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.674798
//+ Revised: 	2026/11/03 04:54:19.451992
// -----------------------------------------------------------------------------

//! The subkeys derived from the master key with BLAKE3's key-derivation mode, one fixed context
//! per purpose, so no key is ever used for two things.
//!
//! The context strings are part of the stored data's format: changing one makes everything
//! encrypted or hashed with that key unreadable. A golden test pins them.

use std::fmt;

use zeroize::Zeroizing;

use super::master_key::MasterKey;

const LICENCE_ENCRYPTION_CONTEXT: &str = "clavren 2026-11-02 licence key encryption v1";
const LICENCE_LOOKUP_CONTEXT: &str = "clavren 2026-11-02 licence key lookup v1";
const TOTP_ENCRYPTION_CONTEXT: &str = "clavren 2026-11-02 admin totp secret encryption v1";
const CSRF_CONTEXT: &str = "clavren 2026-11-02 admin csrf token v1";
const TOKEN_SIGNING_CONTEXT_PREFIX: &str = "clavren 2026-11-02 activation token signing v";

pub struct SubKey(Zeroizing<[u8; 32]>);

impl SubKey {
    fn derive(context: &str, master: &MasterKey) -> Self {
        Self(Zeroizing::new(blake3::derive_key(context, master.as_bytes())))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SubKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubKey(<redacted>)")
    }
}

/// The master key and everything derived from it, shared by the whole server.
#[derive(Debug)]
pub struct KeyRing {
    master: MasterKey,
    licence_encryption: SubKey,
    licence_lookup: SubKey,
    totp_encryption: SubKey,
    csrf: SubKey,
}

impl KeyRing {
    pub fn new(master: MasterKey) -> Self {
        Self {
            licence_encryption: SubKey::derive(LICENCE_ENCRYPTION_CONTEXT, &master),
            licence_lookup: SubKey::derive(LICENCE_LOOKUP_CONTEXT, &master),
            totp_encryption: SubKey::derive(TOTP_ENCRYPTION_CONTEXT, &master),
            csrf: SubKey::derive(CSRF_CONTEXT, &master),
            master,
        }
    }

    /// Encrypts stored licence keys (XChaCha20-Poly1305).
    pub fn licence_encryption(&self) -> &SubKey {
        &self.licence_encryption
    }

    /// Keys the BLAKE3 lookup hash of licence keys.
    pub fn licence_lookup(&self) -> &SubKey {
        &self.licence_lookup
    }

    /// Encrypts admin TOTP secrets.
    pub fn totp_encryption(&self) -> &SubKey {
        &self.totp_encryption
    }

    /// Keys the BLAKE3 hash that turns a session token into its CSRF token.
    pub fn csrf(&self) -> &SubKey {
        &self.csrf
    }

    /// The Ed25519 seed for activation-token signing key `version` (the token's `kid`). Derived
    /// on demand, because rotation adds versions without changing the stored data.
    pub fn token_signing_seed(&self, version: u32) -> SubKey {
        SubKey::derive(&format!("{TOKEN_SIGNING_CONTEXT_PREFIX}{version}"), &self.master)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn ring(byte: u8) -> KeyRing {
        KeyRing::new(MasterKey::from_bytes([byte; 32]))
    }

    fn hex(key: &SubKey) -> String {
        key.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// Pins the derivation. A failure here means stored licence keys, lookups, TOTP secrets or
    /// issued tokens would no longer match: never update these values to make the test pass.
    /// Cross-checked with `b3sum --derive-key "<context>"` over 32 bytes of 0x42.
    #[test]
    fn golden_subkeys_never_change() {
        let keys = ring(0x42);
        assert_eq!(hex(keys.licence_encryption()), GOLDEN_LICENCE_ENCRYPTION);
        assert_eq!(hex(keys.licence_lookup()), GOLDEN_LICENCE_LOOKUP);
        assert_eq!(hex(keys.totp_encryption()), GOLDEN_TOTP_ENCRYPTION);
        assert_eq!(hex(&keys.token_signing_seed(1)), GOLDEN_TOKEN_SIGNING_V1);
        assert_eq!(hex(keys.csrf()), GOLDEN_CSRF);
    }

    const GOLDEN_LICENCE_ENCRYPTION: &str = "32577a9867187c1d2eb1930d7750824b274b3744e1d18433942202d17f334514";
    const GOLDEN_LICENCE_LOOKUP: &str = "de19b0f3349ff82135a39c51e5c4df3e34a3f527c042cb55dde247fd0cda72da";
    const GOLDEN_TOTP_ENCRYPTION: &str = "07a23b2a8b852233230dd5e8b8817ad24298b328dd31c951012fc07f04ce6788";
    const GOLDEN_TOKEN_SIGNING_V1: &str = "294838f9f43ef42786bcfa6017489bb3143571076f5a0b8962939afcf88a3294";
    const GOLDEN_CSRF: &str = "cf7143183f9925d3924e9fb84c6b8e3a09c2077af2e75b13b860ed2d5734e3cb";

    #[test]
    fn the_same_master_key_gives_the_same_subkeys() {
        assert_eq!(ring(1).licence_lookup().as_bytes(), ring(1).licence_lookup().as_bytes());
        assert_eq!(ring(1).token_signing_seed(3).as_bytes(), ring(1).token_signing_seed(3).as_bytes());
    }

    #[test]
    fn debug_output_never_shows_a_key() {
        let printed = format!("{:?}", ring(0x42));
        assert!(!printed.contains("42, 42") && !printed.contains("0x42"), "{printed}");
        assert!(printed.contains("<redacted>"));
    }

    proptest! {
        /// No two purposes, and no two signing-key versions, ever share a key.
        #[test]
        fn every_subkey_is_distinct(master in any::<[u8; 32]>(), version in 1u32..1000) {
            let keys = KeyRing::new(MasterKey::from_bytes(master));
            let all = [
                *keys.licence_encryption().as_bytes(),
                *keys.licence_lookup().as_bytes(),
                *keys.totp_encryption().as_bytes(),
                *keys.csrf().as_bytes(),
                *keys.token_signing_seed(version).as_bytes(),
                *keys.token_signing_seed(version + 1).as_bytes(),
                master,
            ];
            for (index, key) in all.iter().enumerate() {
                prop_assert!(all[index + 1..].iter().all(|other| other != key));
            }
        }

        /// Different master keys never give the same subkey.
        #[test]
        fn different_master_keys_give_different_subkeys(first in any::<[u8; 32]>(), second in any::<[u8; 32]>()) {
            prop_assume!(first != second);
            let (first, second) = (KeyRing::new(MasterKey::from_bytes(first)), KeyRing::new(MasterKey::from_bytes(second)));
            prop_assert_ne!(first.licence_lookup().as_bytes(), second.licence_lookup().as_bytes());
        }
    }
}
