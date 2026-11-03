// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:34.754021
//+ Revised: 	2026/11/03 04:54:17.746841
// -----------------------------------------------------------------------------

//! Crockford Base32: the alphabet of licence keys and admin recovery codes. It leaves out I, L, O
//! and U, so codes read aloud or copied by hand are hard to get wrong.

pub const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const BITS_PER_SYMBOL: usize = 5;

/// Encodes `bytes` most significant bit first; a trailing partial group is padded with zero bits.
pub fn encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity((bytes.len() * 8).div_ceil(BITS_PER_SYMBOL));
    let (mut buffer, mut buffered_bits) = (0u16, 0usize);
    for &byte in bytes {
        buffer = (buffer << 8) | u16::from(byte);
        buffered_bits += 8;
        while buffered_bits >= BITS_PER_SYMBOL {
            buffered_bits -= BITS_PER_SYMBOL;
            encoded.push(symbol(buffer >> buffered_bits));
        }
        buffer &= (1 << buffered_bits) - 1;
    }
    if buffered_bits > 0 {
        encoded.push(symbol(buffer << (BITS_PER_SYMBOL - buffered_bits)));
    }
    encoded
}

fn symbol(value: u16) -> char {
    char::from(ALPHABET[usize::from(value & 0b1_1111)])
}

/// Uppercases `typed`, drops spaces and dashes, and maps the look-alikes Crockford allows for
/// (I and L read as 1, O as 0). Returns `None` if anything else outside the alphabet remains.
pub fn normalize(typed: &str) -> Option<String> {
    typed
        .chars()
        .filter(|character| !matches!(character, ' ' | '-' | '\t'))
        .map(|character| match character.to_ascii_uppercase() {
            'I' | 'L' => Some('1'),
            'O' => Some('0'),
            upper if upper.is_ascii() && ALPHABET.contains(&(upper as u8)) => Some(upper),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn known_values() {
        assert_eq!(encode(&[]), "");
        assert_eq!(encode(&[0]), "00");
        assert_eq!(encode(&[0xFF]), "ZW");
        assert_eq!(encode(&[0xFF; 5]), "ZZZZZZZZ");
        // 00000 00001 00010 00011 00100 00101 00110 00111
        assert_eq!(encode(&[0x00, 0x44, 0x32, 0x14, 0xC7]), "01234567");
    }

    #[test]
    fn normalization_accepts_human_typing() {
        assert_eq!(normalize("ab-cd ef").as_deref(), Some("ABCDEF"));
        assert_eq!(normalize("oil").as_deref(), Some("011"));
        assert_eq!(normalize("").as_deref(), Some(""));
    }

    #[test]
    fn normalization_rejects_other_characters() {
        for typed in ["U", "abc!", "ñ", "AB_CD", "12.3"] {
            assert_eq!(normalize(typed), None, "{typed} accepted");
        }
    }

    proptest! {
        #[test]
        fn encoding_uses_the_alphabet_at_the_expected_length(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
            let encoded = encode(&bytes);
            prop_assert_eq!(encoded.len(), (bytes.len() * 8).div_ceil(5));
            prop_assert!(encoded.bytes().all(|symbol| ALPHABET.contains(&symbol)));
        }

        #[test]
        fn different_bytes_of_the_same_length_encode_differently(first in any::<[u8; 10]>(), second in any::<[u8; 10]>()) {
            prop_assume!(first != second);
            prop_assert_ne!(encode(&first), encode(&second));
        }

        #[test]
        fn an_encoding_normalizes_to_itself_in_any_case_and_grouping(bytes in any::<[u8; 16]>(), lower in any::<bool>()) {
            let encoded = encode(&bytes);
            let typed: String = encoded
                .chars()
                .enumerate()
                .flat_map(|(index, symbol)| {
                    let symbol = if lower { symbol.to_ascii_lowercase() } else { symbol };
                    if index > 0 && index % 4 == 0 { vec!['-', symbol] } else { vec![symbol] }
                })
                .collect();
            prop_assert_eq!(normalize(&typed), Some(encoded));
        }
    }
}
