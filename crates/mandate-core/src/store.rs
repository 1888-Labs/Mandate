//! What the authorization engine needs from storage and from cryptography.
//!
//! The engine depends only on these traits, never on Arkiv or on a
//! specific signature scheme. Storage is untrusted: everything it returns
//! is re-verified.

use crate::capability::ResourceId;
use crate::mandate::{Mandate, MandateId};
use crate::principal::PrincipalId;
use crate::revocation::Revocation;

/// Read access to shared authorization state.
pub trait MandateStore {
    /// Every stored record that claims to be mandate `id`.
    ///
    /// Returns all of them, not just one: a mandate's ID does not cover its
    /// signature, so anyone can publish a copy with the same ID and a bad
    /// signature. The engine picks the copy that verifies.
    fn get_mandates(&self, id: &MandateId) -> Vec<Mandate>;

    fn find_mandates(
        &self,
        subject: &PrincipalId,
        action: &str,
        resource: &ResourceId,
    ) -> Vec<Mandate>;

    /// Every revocation that claims to target `mandate_id`.
    ///
    /// Returns all of them, not just one: storage is untrusted, and a
    /// forged revocation must never be able to hide a real one.
    fn find_revocations(&self, mandate_id: &MandateId) -> Vec<Revocation>;
}

/// Checks that a document's ID matches its content and that it carries a
/// valid signature by its stated issuer.
///
/// `mandate-crypto` provides the real implementation.
pub trait SignatureVerifier {
    fn verify_mandate(&self, mandate: &Mandate) -> bool;

    fn verify_revocation(&self, revocation: &Revocation) -> bool;
}
