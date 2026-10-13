//! Checking issuer signatures.

use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use mandate_core::{
    compute_mandate_id, compute_revocation_id, mandate_digest, revocation_digest, Mandate,
    PrincipalId, Revocation, SignatureVerifier,
};

use crate::error::SignatureError;
use crate::keys::{address_of_verifying_key, is_canonical_address};
use crate::signing::{eip191_hash, SIGNATURE_LEN};

/// `SignatureVerifier` for the authorization engine, backed by
/// `verify_mandate_signature`.
#[derive(Debug, Clone, Copy, Default)]
pub struct EthereumVerifier;

impl SignatureVerifier for EthereumVerifier {
    fn verify_mandate(&self, mandate: &Mandate) -> bool {
        verify_mandate_signature(mandate).is_ok()
    }

    fn verify_revocation(&self, revocation: &Revocation) -> bool {
        verify_revocation_signature(revocation).is_ok()
    }
}

/// Verify that `mandate` was signed by its issuer and its ID is honest.
///
/// The digest is recomputed from the mandate's content. The stored `id`
/// is never trusted: if it does not match the content, verification fails.
pub fn verify_mandate_signature(mandate: &Mandate) -> Result<(), SignatureError> {
    if !is_canonical_address(&mandate.issuer) || !is_canonical_address(&mandate.subject) {
        return Err(SignatureError::NonCanonicalAddress);
    }
    let unsigned = mandate.unsigned();
    if mandate.id != compute_mandate_id(&unsigned) {
        return Err(SignatureError::IdMismatch);
    }
    let signer = recover_signer(&mandate_digest(&unsigned), &mandate.signature)?;
    if signer != mandate.issuer {
        return Err(SignatureError::IssuerMismatch);
    }
    Ok(())
}

/// Verify that `revocation` was signed by its stated issuer and its ID is
/// honest.
///
/// This does not decide whether that issuer may revoke the target mandate.
/// The authorization engine does that.
pub fn verify_revocation_signature(revocation: &Revocation) -> Result<(), SignatureError> {
    if !is_canonical_address(&revocation.issuer) {
        return Err(SignatureError::NonCanonicalAddress);
    }
    let unsigned = revocation.unsigned();
    if revocation.id != compute_revocation_id(&unsigned) {
        return Err(SignatureError::IdMismatch);
    }
    let signer = recover_signer(&revocation_digest(&unsigned), &revocation.signature)?;
    if signer != revocation.issuer {
        return Err(SignatureError::IssuerMismatch);
    }
    Ok(())
}

/// Recover the address that `personal_sign`-ed a 32-byte message.
///
/// Accepts `v` as 27/28 (Ethereum) or 0/1 (raw recovery ID). Rejects
/// high-`s` signatures so each signer has one valid signature per message.
pub fn recover_signer(message: &[u8; 32], signature: &[u8]) -> Result<PrincipalId, SignatureError> {
    if signature.len() != SIGNATURE_LEN {
        return Err(SignatureError::MalformedSignature);
    }
    let recovery_byte = match signature[64] {
        v @ (0 | 1) => v,
        v @ (27 | 28) => v - 27,
        _ => return Err(SignatureError::MalformedSignature),
    };
    let recovery_id =
        RecoveryId::from_byte(recovery_byte).ok_or(SignatureError::MalformedSignature)?;
    let sig =
        Signature::from_slice(&signature[..64]).map_err(|_| SignatureError::MalformedSignature)?;
    if sig.normalize_s().is_some() {
        return Err(SignatureError::MalformedSignature);
    }
    let key = VerifyingKey::recover_from_prehash(&eip191_hash(message), &sig, recovery_id)
        .map_err(|_| SignatureError::Unrecoverable)?;
    Ok(address_of_verifying_key(&key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{address_of, SigningKey};
    use crate::signing::{personal_sign, sign_mandate, sign_revocation};
    use mandate_core::{Constraints, UnsignedMandate, UnsignedRevocation};

    fn key(n: u8) -> SigningKey {
        let mut bytes = [0u8; 32];
        bytes[31] = n;
        SigningKey::from_slice(&bytes).unwrap()
    }

    fn alice() -> SigningKey {
        key(1)
    }
    fn mallory() -> SigningKey {
        key(2)
    }

    fn unsigned() -> UnsignedMandate {
        UnsignedMandate {
            issuer: address_of(&alice()),
            subject: address_of(&key(3)),
            action: "transfer".into(),
            resource: "wallet:treasury-demo".parse().unwrap(),
            constraints: Constraints {
                max_amount: Some(1000),
                allowed_recipients: Some(vec!["AWS".into()]),
            },
            parent: None,
            delegable: true,
            issued_at: 1_760_000_000,
            expires_at: 1_760_172_800,
            nonce: 1,
        }
    }

    fn signed() -> Mandate {
        sign_mandate(&unsigned(), &alice()).unwrap()
    }

    /// Change a signed mandate's content and recompute its ID, keeping the
    /// original signature. This is what a dishonest storage operator would do.
    fn tamper(change: impl FnOnce(&mut UnsignedMandate)) -> Mandate {
        let original = signed();
        let mut u = original.unsigned();
        change(&mut u);
        u.into_mandate(original.signature)
    }

    // --- Milestone 4 required tests ---

    #[test]
    fn valid_signature_is_accepted() {
        let m = signed();
        assert_eq!(m.signature.len(), SIGNATURE_LEN);
        assert_eq!(verify_mandate_signature(&m), Ok(()));
    }

    #[test]
    fn forged_signature_is_rejected() {
        // Mallory signs a mandate that claims Alice as its issuer.
        let digest = mandate_digest(&unsigned());
        let forged = personal_sign(&digest, &mallory()).unwrap();
        let m = unsigned().into_mandate(forged.to_vec());
        assert_eq!(
            verify_mandate_signature(&m),
            Err(SignatureError::IssuerMismatch)
        );

        // Random bytes are not a signature.
        let mut m = signed();
        m.signature = vec![0xab; SIGNATURE_LEN];
        assert!(verify_mandate_signature(&m).is_err());
    }

    #[test]
    fn modified_amount_breaks_signature() {
        let m = tamper(|u| u.constraints.max_amount = Some(5000));
        assert_eq!(
            verify_mandate_signature(&m),
            Err(SignatureError::IssuerMismatch)
        );
    }

    #[test]
    fn modified_subject_breaks_signature() {
        let m = tamper(|u| u.subject = address_of(&mallory()));
        assert_eq!(
            verify_mandate_signature(&m),
            Err(SignatureError::IssuerMismatch)
        );
    }

    #[test]
    fn modified_resource_breaks_signature() {
        let m = tamper(|u| u.resource = "wallet:another-wallet".parse().unwrap());
        assert_eq!(
            verify_mandate_signature(&m),
            Err(SignatureError::IssuerMismatch)
        );
    }

    #[test]
    fn wrong_issuer_is_rejected() {
        // Alice's valid signature, relabelled as coming from Mallory.
        let m = tamper(|u| u.issuer = address_of(&mallory()));
        assert_eq!(
            verify_mandate_signature(&m),
            Err(SignatureError::IssuerMismatch)
        );

        // Signing with a key that is not the issuer's is refused up front.
        assert_eq!(
            sign_mandate(&unsigned(), &mallory()),
            Err(SignatureError::IssuerMismatch)
        );
    }

    // --- ID integrity ---

    #[test]
    fn modified_content_with_stale_id_is_rejected() {
        let mut m = signed();
        m.constraints.max_amount = Some(5000);
        assert_eq!(
            verify_mandate_signature(&m),
            Err(SignatureError::IdMismatch)
        );
    }

    #[test]
    fn substituted_id_is_rejected() {
        let mut m = signed();
        m.id = mandate_core::MandateId(format!("0x{}", "00".repeat(32)));
        assert_eq!(
            verify_mandate_signature(&m),
            Err(SignatureError::IdMismatch)
        );
    }

    // --- Signature format ---

    #[test]
    fn wrong_length_signature_is_rejected() {
        for len in [0, 64, 66] {
            let mut m = signed();
            m.signature.resize(len, 0);
            assert_eq!(
                verify_mandate_signature(&m),
                Err(SignatureError::MalformedSignature),
                "len {len}"
            );
        }
    }

    #[test]
    fn invalid_recovery_byte_is_rejected() {
        for v in [2, 26, 29, 255] {
            let mut m = signed();
            m.signature[64] = v;
            assert_eq!(
                verify_mandate_signature(&m),
                Err(SignatureError::MalformedSignature),
                "v {v}"
            );
        }
    }

    #[test]
    fn raw_recovery_id_is_accepted() {
        let mut m = signed();
        m.signature[64] -= 27;
        assert_eq!(verify_mandate_signature(&m), Ok(()));
    }

    #[test]
    fn high_s_signature_is_rejected() {
        // (r, n - s, v ^ 1) is the same signature in its malleable form.
        let m = signed();
        let sig = Signature::from_slice(&m.signature[..64]).unwrap();
        let (r, s) = sig.split_scalars();
        let high = Signature::from_scalars(r, -*s).unwrap();
        let mut bytes = high.to_bytes().to_vec();
        bytes.push(m.signature[64] ^ 1);
        let mut m = m;
        m.signature = bytes;
        assert_eq!(
            verify_mandate_signature(&m),
            Err(SignatureError::MalformedSignature)
        );
    }

    #[test]
    fn non_canonical_addresses_are_rejected() {
        let mut u = unsigned();
        u.issuer = mandate_core::PrincipalId(u.issuer.0.to_ascii_uppercase().replace("0X", "0x"));
        assert_eq!(
            sign_mandate(&u, &alice()),
            Err(SignatureError::NonCanonicalAddress)
        );

        let mut m = signed();
        m.subject = mandate_core::PrincipalId("finance-agent".into());
        assert_eq!(
            verify_mandate_signature(&m),
            Err(SignatureError::NonCanonicalAddress)
        );
    }

    #[test]
    fn signing_is_deterministic() {
        assert_eq!(signed(), signed());
    }

    #[test]
    fn matches_foundry_personal_sign() {
        // Reference produced with Foundry 1.6.0:
        //   cast wallet sign --private-key 0x00..01 0x1111..11
        // which personal_signs the 32 raw bytes, as a wallet would.
        let message = [0x11u8; 32];
        assert_eq!(
            hex::encode(eip191_hash(&message)),
            "245a48de257ae28de2b11cb8fc02361fe87a20566dc63bec4492c3854b1aae52"
        );
        let expected = "625704c33dc9ef0c81f521cfac3a731f33800a8a7a99341b638d8e0c64607a3c\
                        285d904e2355a2c7da1b4e53673028fcd2de2553f845475df3e191d9c4a87437\
                        1c";
        let signature = personal_sign(&message, &alice()).unwrap();
        assert_eq!(hex::encode(signature), expected);
        assert_eq!(
            recover_signer(&message, &signature),
            Ok(address_of(&alice()))
        );
    }
    // --- Revocations ---

    fn unsigned_revocation() -> UnsignedRevocation {
        UnsignedRevocation {
            mandate_id: signed().id,
            issuer: address_of(&alice()),
            revoked_at: 1_760_003_600,
            reason: Some("user_revoked".into()),
        }
    }

    #[test]
    fn valid_revocation_is_accepted() {
        let r = sign_revocation(&unsigned_revocation(), &alice()).unwrap();
        assert_eq!(verify_revocation_signature(&r), Ok(()));
    }

    #[test]
    fn forged_revocation_signature_is_rejected() {
        // Mallory signs a revocation that claims Alice as its issuer.
        let forged = personal_sign(&revocation_digest(&unsigned_revocation()), &mallory()).unwrap();
        let r = unsigned_revocation().into_revocation(forged.to_vec());
        assert_eq!(
            verify_revocation_signature(&r),
            Err(SignatureError::IssuerMismatch)
        );
        assert_eq!(
            sign_revocation(&unsigned_revocation(), &mallory()),
            Err(SignatureError::IssuerMismatch)
        );
    }

    #[test]
    fn retargeted_revocation_is_rejected() {
        let real = sign_revocation(&unsigned_revocation(), &alice()).unwrap();
        let mut u = real.unsigned();
        u.mandate_id = mandate_core::MandateId(format!("0x{}", "ab".repeat(32)));
        let moved = u.into_revocation(real.signature.clone());
        assert_eq!(
            verify_revocation_signature(&moved),
            Err(SignatureError::IssuerMismatch)
        );

        let mut stale = real;
        stale.mandate_id = mandate_core::MandateId("0x02".into());
        assert_eq!(
            verify_revocation_signature(&stale),
            Err(SignatureError::IdMismatch)
        );
    }

    #[test]
    fn mandate_signature_cannot_be_reused_as_revocation() {
        // Domain separation: Alice's signature on M1 is not a revocation.
        let m = signed();
        let r = unsigned_revocation().into_revocation(m.signature.clone());
        assert_eq!(
            verify_revocation_signature(&r),
            Err(SignatureError::IssuerMismatch)
        );
    }
}
