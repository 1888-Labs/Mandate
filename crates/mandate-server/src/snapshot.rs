//! Gather everything one decision needs from the backend into a
//! `MemoryMandateStore`, then let the synchronous engine decide.
//!
//! Fetched: the starting mandates, every stored copy of every ancestor (up
//! to the engine's depth limit, one batched query per level), and every
//! revocation of any of them, published by that mandate's issuer (one
//! batched query). For the demo chain that is three Arkiv queries per
//! decision.
//!
//! Only documents whose Arkiv `$creator` is their own issuer are used: a
//! copy of a mandate or revocation published by anyone else is ignored. The
//! adapter already filters this way. It is checked again here, so the rule
//! does not depend on the adapter alone.
//!
//! Anything not fetched is simply missing, which the engine treats as a
//! denial, so a partial snapshot can never produce an ALLOW.

use std::collections::{BTreeMap, BTreeSet};

use mandate_core::authorization::MAX_CHAIN_DEPTH;
use mandate_core::{
    AuthorizationRequest, Mandate, MandateId, MemoryMandateStore, PrincipalId, Revocation,
    RevocationId,
};

use crate::backend::{ArkivRef, Backend, Result, Stored};

/// Upper bound on mandates pulled into one snapshot.
const MAX_MANDATES: usize = 512;

/// The documents one decision is based on, with where each lives on Arkiv.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub store: MemoryMandateStore,
    pub mandates: BTreeMap<MandateId, ArkivRef>,
    pub revocations: BTreeMap<RevocationId, ArkivRef>,
}

/// Published by its own issuer. Backends that don't track creators
/// (`creator: None`) are trusted to have filtered already.
fn issuer_published(creator: &Option<PrincipalId>, issuer: &PrincipalId) -> bool {
    creator.as_ref().is_none_or(|c| c == issuer)
}

impl Snapshot {
    fn add_mandate(&mut self, s: Stored<Mandate>) -> bool {
        if !issuer_published(&s.arkiv.creator, &s.doc.issuer) {
            return false;
        }
        self.mandates.entry(s.doc.id.clone()).or_insert(s.arkiv);
        self.store.insert_mandate(s.doc);
        true
    }

    fn add_revocation(&mut self, s: Stored<Revocation>) {
        if issuer_published(&s.arkiv.creator, &s.doc.issuer) {
            self.revocations.entry(s.doc.id.clone()).or_insert(s.arkiv);
            self.store.insert_revocation(s.doc);
        }
    }
}

/// Candidates for `request` and their chains.
pub async fn for_request<B: Backend>(
    backend: &B,
    request: &AuthorizationRequest,
) -> Result<Snapshot> {
    let leaves = backend
        .mandates_for_subject(
            &request.subject,
            Some(&request.action),
            Some(&request.resource),
            None,
        )
        .await?;
    for_leaves(backend, leaves).await
}

/// Every copy of mandate `id` and their chains.
pub async fn for_mandate<B: Backend>(backend: &B, id: &MandateId) -> Result<Snapshot> {
    let copies = backend.mandates_by_ids(std::slice::from_ref(id)).await?;
    for_leaves(backend, copies).await
}

pub async fn for_leaves<B: Backend>(backend: &B, leaves: Vec<Stored<Mandate>>) -> Result<Snapshot> {
    let mut snap = Snapshot::default();
    let mut seen: BTreeSet<MandateId> = BTreeSet::new();
    let mut frontier: BTreeSet<MandateId> = BTreeSet::new();

    for m in leaves.into_iter().take(MAX_MANDATES) {
        seen.insert(m.doc.id.clone());
        let parent = m.doc.parent.clone();
        if snap.add_mandate(m) {
            frontier.extend(parent);
        }
    }

    for _ in 0..=MAX_CHAIN_DEPTH {
        frontier.retain(|id| !seen.contains(id));
        if frontier.is_empty() || snap.store.mandates().len() >= MAX_MANDATES {
            break;
        }
        let ids: Vec<MandateId> = std::mem::take(&mut frontier).into_iter().collect();
        let fetched = backend.mandates_by_ids(&ids).await?;
        seen.extend(ids);
        for m in fetched {
            let parent = m.doc.parent.clone();
            if snap.add_mandate(m) {
                frontier.extend(parent);
            }
        }
    }

    let targets: BTreeSet<(MandateId, PrincipalId)> = snap
        .store
        .mandates()
        .iter()
        .map(|m| (m.id.clone(), m.issuer.clone()))
        .collect();
    let targets: Vec<_> = targets.into_iter().collect();
    if !targets.is_empty() {
        for r in backend.revocations_for(&targets).await? {
            snap.add_revocation(r);
        }
    }
    Ok(snap)
}
