//! In-memory authorization state.
//!
//! Mirrors how shared storage (Arkiv) behaves: it accepts any record and
//! verifies nothing. Trust comes only from the authorization engine.

use crate::capability::ResourceId;
use crate::mandate::{Mandate, MandateId};
use crate::principal::PrincipalId;
use crate::revocation::Revocation;
use crate::store::MandateStore;

/// A `MandateStore` held in memory, for tests, demos and local use.
///
/// Lookups are linear scans. That's fine at demo scale; it isn't meant
/// for large datasets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryMandateStore {
    mandates: Vec<Mandate>,
    revocations: Vec<Revocation>,
}

impl MemoryMandateStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a mandate. An exact duplicate (same ID and signature) is a
    /// no-op. Other records with the same ID are kept alongside it.
    pub fn insert_mandate(&mut self, mandate: Mandate) {
        let duplicate = self
            .mandates
            .iter()
            .any(|m| m.id == mandate.id && m.signature == mandate.signature);
        if !duplicate {
            self.mandates.push(mandate);
        }
    }

    /// Store a revocation. An exact duplicate is a no-op.
    pub fn insert_revocation(&mut self, revocation: Revocation) {
        let duplicate = self
            .revocations
            .iter()
            .any(|r| r.id == revocation.id && r.signature == revocation.signature);
        if !duplicate {
            self.revocations.push(revocation);
        }
    }

    pub fn mandates(&self) -> &[Mandate] {
        &self.mandates
    }

    pub fn revocations(&self) -> &[Revocation] {
        &self.revocations
    }

    /// Mandates delegated directly from `parent`.
    pub fn find_children(&self, parent: &MandateId) -> Vec<Mandate> {
        self.mandates
            .iter()
            .filter(|m| m.parent.as_ref() == Some(parent))
            .cloned()
            .collect()
    }
}

impl MandateStore for MemoryMandateStore {
    fn get_mandates(&self, id: &MandateId) -> Vec<Mandate> {
        self.mandates
            .iter()
            .filter(|m| &m.id == id)
            .cloned()
            .collect()
    }

    fn find_mandates(
        &self,
        subject: &PrincipalId,
        action: &str,
        resource: &ResourceId,
    ) -> Vec<Mandate> {
        self.mandates
            .iter()
            .filter(|m| &m.subject == subject && m.action == action && &m.resource == resource)
            .cloned()
            .collect()
    }

    fn find_revocations(&self, mandate_id: &MandateId) -> Vec<Revocation> {
        self.revocations
            .iter()
            .filter(|r| &r.mandate_id == mandate_id)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Constraints;
    use crate::mandate::UnsignedMandate;
    use crate::revocation::UnsignedRevocation;

    fn mandate(subject: &str, action: &str, resource: &str, nonce: u64) -> Mandate {
        UnsignedMandate {
            issuer: PrincipalId("alice".into()),
            subject: PrincipalId(subject.into()),
            action: action.into(),
            resource: resource.parse().unwrap(),
            constraints: Constraints::default(),
            parent: None,
            delegable: true,
            issued_at: 0,
            expires_at: 100,
            nonce,
        }
        .into_mandate(b"sig".to_vec())
    }

    fn revocation(of: &Mandate, reason: &str) -> Revocation {
        UnsignedRevocation {
            mandate_id: of.id.clone(),
            issuer: of.issuer.clone(),
            revoked_at: 50,
            reason: Some(reason.into()),
        }
        .into_revocation(b"sig".to_vec())
    }

    #[test]
    fn finds_mandates_by_subject_action_and_resource() {
        let mut store = MemoryMandateStore::new();
        let hit = mandate("bob", "transfer", "wallet:a", 1);
        store.insert_mandate(hit.clone());
        store.insert_mandate(mandate("carol", "transfer", "wallet:a", 2));
        store.insert_mandate(mandate("bob", "read", "wallet:a", 3));
        store.insert_mandate(mandate("bob", "transfer", "wallet:b", 4));

        let found = store.find_mandates(
            &PrincipalId("bob".into()),
            "transfer",
            &"wallet:a".parse().unwrap(),
        );
        assert_eq!(found, vec![hit]);
    }

    #[test]
    fn gets_every_copy_of_a_mandate_id() {
        let mut store = MemoryMandateStore::new();
        let real = mandate("bob", "transfer", "wallet:a", 1);
        let mut copy = real.clone();
        copy.signature = b"garbage".to_vec();
        store.insert_mandate(real.clone());
        store.insert_mandate(copy.clone());
        assert_eq!(store.get_mandates(&real.id), vec![real, copy]);
        assert!(store
            .get_mandates(&MandateId("0xmissing".into()))
            .is_empty());
    }

    #[test]
    fn exact_duplicates_are_stored_once() {
        let mut store = MemoryMandateStore::new();
        let m = mandate("bob", "transfer", "wallet:a", 1);
        let r = revocation(&m, "user_revoked");
        store.insert_mandate(m.clone());
        store.insert_mandate(m.clone());
        store.insert_revocation(r.clone());
        store.insert_revocation(r);
        assert_eq!(store.mandates().len(), 1);
        assert_eq!(store.revocations().len(), 1);
    }

    #[test]
    fn finds_every_revocation_of_a_mandate() {
        let mut store = MemoryMandateStore::new();
        let m = mandate("bob", "transfer", "wallet:a", 1);
        let other = mandate("bob", "transfer", "wallet:a", 2);
        let r1 = revocation(&m, "user_revoked");
        let r2 = revocation(&m, "compromised");
        store.insert_revocation(r1.clone());
        store.insert_revocation(revocation(&other, "user_revoked"));
        store.insert_revocation(r2.clone());
        assert_eq!(store.find_revocations(&m.id), vec![r1, r2]);
    }

    #[test]
    fn finds_children_of_a_mandate() {
        let mut store = MemoryMandateStore::new();
        let parent = mandate("bob", "transfer", "wallet:a", 1);
        let child = UnsignedMandate {
            issuer: parent.subject.clone(),
            subject: PrincipalId("carol".into()),
            parent: Some(parent.id.clone()),
            nonce: 2,
            ..parent.unsigned()
        }
        .into_mandate(b"sig".to_vec());
        store.insert_mandate(parent.clone());
        store.insert_mandate(child.clone());
        assert_eq!(store.find_children(&parent.id), vec![child]);
        assert!(store.find_children(&MandateId("0xnone".into())).is_empty());
    }
}
