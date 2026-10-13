//! Gather everything one decision needs from the backend into a
//! `MemoryMandateStore`, then let the synchronous engine decide.
//!
//! Fetched: the starting mandates, every stored copy of every ancestor (up
//! to the engine's depth limit, one batched query per level), and every
//! revocation of any of them (one batched query). For the demo chain that
//! is three Arkiv queries per decision.
//! Anything not fetched is simply missing, which the engine treats as a
//! denial, so a partial snapshot can never produce an ALLOW.

use std::collections::BTreeSet;

use mandate_core::authorization::MAX_CHAIN_DEPTH;
use mandate_core::{AuthorizationRequest, Mandate, MandateId, MemoryMandateStore};

use crate::backend::{Backend, Result};

/// Upper bound on mandates pulled into one snapshot.
const MAX_MANDATES: usize = 512;

/// Candidates for `request` and their chains.
pub async fn for_request<B: Backend>(
    backend: &B,
    request: &AuthorizationRequest,
) -> Result<MemoryMandateStore> {
    let leaves = backend
        .mandates_for_subject(
            &request.subject,
            Some(&request.action),
            Some(&request.resource),
        )
        .await?;
    for_leaves(backend, leaves).await
}

/// Every copy of mandate `id` and their chains.
pub async fn for_mandate<B: Backend>(backend: &B, id: &MandateId) -> Result<MemoryMandateStore> {
    let copies = backend.mandates_by_ids(std::slice::from_ref(id)).await?;
    for_leaves(backend, copies).await
}

pub async fn for_leaves<B: Backend>(
    backend: &B,
    leaves: Vec<Mandate>,
) -> Result<MemoryMandateStore> {
    let mut store = MemoryMandateStore::new();
    let mut seen: BTreeSet<MandateId> = BTreeSet::new();
    let mut frontier: BTreeSet<MandateId> = BTreeSet::new();

    for m in leaves.into_iter().take(MAX_MANDATES) {
        seen.insert(m.id.clone());
        frontier.extend(m.parent.clone());
        store.insert_mandate(m);
    }

    for _ in 0..=MAX_CHAIN_DEPTH {
        frontier.retain(|id| !seen.contains(id));
        if frontier.is_empty() || store.mandates().len() >= MAX_MANDATES {
            break;
        }
        let ids: Vec<MandateId> = std::mem::take(&mut frontier).into_iter().collect();
        let fetched = backend.mandates_by_ids(&ids).await?;
        seen.extend(ids);
        for m in fetched {
            frontier.extend(m.parent.clone());
            store.insert_mandate(m);
        }
    }

    let ids: BTreeSet<MandateId> = store.mandates().iter().map(|m| m.id.clone()).collect();
    let ids: Vec<MandateId> = ids.into_iter().collect();
    for r in backend.revocations_for(&ids).await? {
        store.insert_revocation(r);
    }
    Ok(store)
}
