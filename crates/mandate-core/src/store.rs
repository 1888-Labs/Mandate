//! What the authorization engine needs from storage and from cryptography.
//!
//! The engine depends only on these traits, never on Arkiv or on a
//! specific signature scheme. Storage is untrusted: everything it returns
//! is re-verified.

use crate::capability::ResourceId;
use crate::mandate::{Mandate, MandateId};
use crate::principal::PrincipalId;

/// Read access to shared authorization state.
pub trait MandateStore {
    fn get_mandate(&self, id: &MandateId) -> Option<Mandate>;

    fn find_mandates(
        &self,
        subject: &PrincipalId,
        action: &str,
        resource: &ResourceId,
    ) -> Vec<Mandate>;
}

/// Checks that a mandate's ID matches its content and that it carries a
/// valid signature by its issuer.
///
/// `mandate-crypto` provides the real implementation.
pub trait SignatureVerifier {
    fn verify_mandate(&self, mandate: &Mandate) -> bool;
}
