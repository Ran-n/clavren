// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:10.834248
//+ Revised: 	2026/11/03 04:54:10.834248
// -----------------------------------------------------------------------------

//! Windows DPAPI in CurrentUser scope: data protected here can only be read back by the same
//! Windows account on the same computer. The product code is added as entropy, so one product's
//! file can't be passed off as another's.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

use crate::store::{Protector, StoreError};

pub struct Dpapi {
    entropy: Vec<u8>,
}

impl Dpapi {
    pub fn new(product: &str) -> Self {
        Self { entropy: format!("clavren:{product}").into_bytes() }
    }
}

fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    // DPAPI only reads input blobs; the pointer is `*mut` in the declaration only
    CRYPT_INTEGER_BLOB { cbData: bytes.len() as u32, pbData: bytes.as_ptr().cast_mut() }
}

/// Copies the output blob DPAPI allocated, then frees it.
#[allow(unsafe_code)]
fn take(output: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    // SAFETY: on success DPAPI sets `pbData` to a LocalAlloc'd buffer of `cbData` bytes, which is
    // read once here and freed exactly once.
    unsafe {
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        bytes
    }
}

impl Protector for Dpapi {
    #[allow(unsafe_code)]
    fn protect(&self, plain: &[u8]) -> Result<Vec<u8>, StoreError> {
        let (input, entropy) = (blob(plain), blob(&self.entropy));
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
        // SAFETY: every pointer is to a live local; DPAPI fills `output` on success.
        let ok = unsafe {
            CryptProtectData(&input, null(), &entropy, null(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        };
        if ok == 0 {
            return Err(StoreError::Protect(std::io::Error::last_os_error()));
        }
        Ok(take(output))
    }

    #[allow(unsafe_code)]
    fn unprotect(&self, protected: &[u8]) -> Result<Vec<u8>, StoreError> {
        let (input, entropy) = (blob(protected), blob(&self.entropy));
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
        // SAFETY: as above; no description is asked for.
        let ok = unsafe {
            CryptUnprotectData(&input, null_mut(), &entropy, null(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        };
        if ok == 0 {
            return Err(StoreError::Unprotect(std::io::Error::last_os_error()));
        }
        Ok(take(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_data_reads_back_with_the_same_product_only() {
        let protected = Dpapi::new("avalio-lite").protect(b"device key").unwrap();
        assert!(!protected.windows(10).any(|window| window == b"device key"));
        assert_eq!(Dpapi::new("avalio-lite").unprotect(&protected).unwrap(), b"device key");
        assert!(matches!(Dpapi::new("other").unprotect(&protected), Err(StoreError::Unprotect(_))));
        assert!(Dpapi::new("avalio-lite").unprotect(b"not protected").is_err());
    }
}
