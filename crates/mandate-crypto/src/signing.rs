//! Producing issuer signatures.

use mandate_core::{
    mandate_digest, revocation_digest, Mandate, Revocation, UnsignedMandate, UnsignedRevocation,
};
use sha3::{Digest, Keccak256};

use crate::error::SignatureError;
use crate::keys::{address_of, is_canonical_address, SigningKey};

/// `r || s || v`, with `v` in {27, 28} as in Ethereum `personal_sign`.
pub const SIGNATURE_LEN: usize = 65;

/// EIP-191 `personal_sign` hash of a 32-byte message:
/// `keccak256("\x19Ethereum Signed Message:\n32" || message)`.
pub(crate) fn eip191_hash(message: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(b"\x19Ethereum Signed Message:\n32");
    hasher.update(message);
    hasher.finalize().into()
}

/// Sign a 32-byte message the way an Ethereum wallet's `personal_sign` would.
pub(crate) fn personal_sign(
    message: &[u8; 32],
    key: &SigningKey,
) -> Result<[u8; SIGNATURE_LEN], SignatureError> {
    let (signature, recovery_id) = key
        .sign_prehash_recoverable(&eip191_hash(message))
        .map_err(|_| SignatureError::SigningFailed)?;
    let mut out = [0u8; SIGNATURE_LEN];
    out[..64].copy_from_slice(&signature.to_bytes());
    out[64] = 27 + recovery_id.to_byte();
    Ok(out)
}

/// Sign `mandate` as its issuer and return the complete signed mandate.
///
/// Refuses to sign if `key` does not belong to `mandate.issuer`, so a
/// mandate that would fail verification is never produced.
pub fn sign_mandate(
    mandate: &UnsignedMandate,
    key: &SigningKey,
) -> Result<Mandate, SignatureError> {
    if !is_canonical_address(&mandate.issuer) || !is_canonical_address(&mandate.subject) {
        return Err(SignatureError::NonCanonicalAddress);
    }
    if address_of(key) != mandate.issuer {
        return Err(SignatureError::IssuerMismatch);
    }
    let signature = personal_sign(&mandate_digest(mandate), key)?;
    Ok(mandate.clone().into_mandate(signature.to_vec()))
}

/// Sign `revocation` as its issuer and return the signed revocation.
///
/// Refuses if `key` does not belong to `revocation.issuer`. Whether that
/// issuer is *allowed* to revoke the target mandate is decided by the
/// authorization engine (v0.1: only the mandate's own issuer).
pub fn sign_revocation(
    revocation: &UnsignedRevocation,
    key: &SigningKey,
) -> Result<Revocation, SignatureError> {
    if !is_canonical_address(&revocation.issuer) {
        return Err(SignatureError::NonCanonicalAddress);
    }
    if address_of(key) != revocation.issuer {
        return Err(SignatureError::IssuerMismatch);
    }
    let signature = personal_sign(&revocation_digest(revocation), key)?;
    Ok(revocation.clone().into_revocation(signature.to_vec()))
}
