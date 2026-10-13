//! secp256k1 keys and Ethereum addresses.
//!
//! Principals are identified by their address in canonical form:
//! `0x` followed by 40 lowercase hex digits. EIP-55 mixed-case checksums
//! are accepted as input by `parse_address` but never stored.

pub use k256::ecdsa::SigningKey;
use k256::ecdsa::VerifyingKey;
use mandate_core::PrincipalId;
use rand_core::OsRng;
use sha3::{Digest, Keccak256};

use crate::error::SignatureError;

/// A new random signing key from the operating system's RNG.
pub fn generate_signing_key() -> SigningKey {
    SigningKey::random(&mut OsRng)
}

/// The canonical Ethereum address of a signing key.
pub fn address_of(key: &SigningKey) -> PrincipalId {
    address_of_verifying_key(key.verifying_key())
}

pub(crate) fn address_of_verifying_key(key: &VerifyingKey) -> PrincipalId {
    let point = key.to_encoded_point(false);
    // Uncompressed SEC1 is 0x04 || x || y; the address hashes x || y.
    let hash = Keccak256::digest(&point.as_bytes()[1..]);
    PrincipalId(format!("0x{}", hex::encode(&hash[12..])))
}

/// Whether `p` is a lowercase `0x`-prefixed 20-byte hex address.
pub fn is_canonical_address(p: &PrincipalId) -> bool {
    match p.0.strip_prefix("0x") {
        Some(digits) => {
            digits.len() == 40
                && digits
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        }
        None => false,
    }
}

/// Parse an address in any letter case into its canonical form.
pub fn parse_address(s: &str) -> Result<PrincipalId, SignatureError> {
    let p = PrincipalId(s.to_ascii_lowercase());
    if is_canonical_address(&p) {
        Ok(p)
    } else {
        Err(SignatureError::NonCanonicalAddress)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_matches_known_ethereum_vector() {
        // Private key 1 → 0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf.
        let mut bytes = [0u8; 32];
        bytes[31] = 1;
        let key = SigningKey::from_slice(&bytes).unwrap();
        assert_eq!(
            address_of(&key),
            PrincipalId("0x7e5f4552091a69125d5dfcb7b8c2659029395bdf".into())
        );
    }

    #[test]
    fn generated_keys_have_canonical_distinct_addresses() {
        let a = address_of(&generate_signing_key());
        let b = address_of(&generate_signing_key());
        assert!(is_canonical_address(&a));
        assert!(is_canonical_address(&b));
        assert_ne!(a, b);
    }

    #[test]
    fn parse_address_lowercases_checksummed_input() {
        assert_eq!(
            parse_address("0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf"),
            Ok(PrincipalId(
                "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf".into()
            ))
        );
    }

    #[test]
    fn non_canonical_addresses_are_rejected() {
        for bad in [
            "7e5f4552091a69125d5dfcb7b8c2659029395bdf",    // no 0x
            "0x7e5f4552091a69125d5dfcb7b8c2659029395bd",   // 39 digits
            "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf0", // 41 digits
            "0x7e5f4552091a69125d5dfcb7b8c2659029395bdg",  // not hex
            "alice",
            "",
        ] {
            assert_eq!(
                parse_address(bad),
                Err(SignatureError::NonCanonicalAddress),
                "{bad}"
            );
        }
        assert!(!is_canonical_address(&PrincipalId(
            "0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf".into()
        )));
    }
}
