// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.249341
//+ Revised: 	2026/11/03 04:54:12.249341
// -----------------------------------------------------------------------------

//! Binary values on the wire: unpadded base64url.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

pub fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn decode(text: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(text).ok()
}

/// Exactly `N` bytes, or `None`.
pub fn decode_array<const N: usize>(text: &str) -> Option<[u8; N]> {
    decode(text)?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip_without_padding() {
        assert_eq!(encode(&[0xFB, 0xFF]), "-_8");
        assert_eq!(decode("-_8"), Some(vec![0xFB, 0xFF]));
        assert_eq!(decode("-_8="), None, "padding is refused");
        assert_eq!(decode_array::<2>("-_8"), Some([0xFB, 0xFF]));
        assert_eq!(decode_array::<3>("-_8"), None);
    }
}
