//! Where authorization state lives. The real backend is Arkiv, through the
//! adapter; tests use an in-memory one. Neither is trusted: everything read
//! back is re-verified by the engine.

use std::fmt;
use std::future::Future;
use std::sync::Mutex;

use mandate_core::{
    Mandate, MandateId, MandateStore, MemoryMandateStore, PrincipalId, ResourceId, Revocation,
};
use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Published {
    /// Arkiv entity key, when stored on Arkiv.
    pub entity_key: Option<String>,
    pub tx_hash: Option<String>,
    /// False if the exact document was already stored.
    pub created: bool,
}

/// The backend could not be reached or misbehaved. Authorization must
/// fail closed on this, never allow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendError(pub String);

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BackendError {}

pub type Result<T> = std::result::Result<T, BackendError>;

/// Async read/write access to shared authorization state. Every lookup
/// returns *all* matching records, mirroring `MandateStore`.
pub trait Backend: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    /// Every stored copy of any of `ids`.
    fn mandates_by_ids(
        &self,
        ids: &[MandateId],
    ) -> impl Future<Output = Result<Vec<Mandate>>> + Send;

    fn mandates_for_subject(
        &self,
        subject: &PrincipalId,
        action: Option<&str>,
        resource: Option<&ResourceId>,
    ) -> impl Future<Output = Result<Vec<Mandate>>> + Send;

    /// Every revocation that claims any of `mandate_ids`.
    fn revocations_for(
        &self,
        mandate_ids: &[MandateId],
    ) -> impl Future<Output = Result<Vec<Revocation>>> + Send;

    fn publish_mandate(&self, mandate: &Mandate) -> impl Future<Output = Result<Published>> + Send;

    /// Store a revocation, keeping it at least until `retain_until`.
    fn publish_revocation(
        &self,
        revocation: &Revocation,
        retain_until: u64,
    ) -> impl Future<Output = Result<Published>> + Send;
}

/// In-process state, for tests and running without Arkiv.
#[derive(Debug, Default)]
pub struct MemoryBackend(Mutex<MemoryMandateStore>);

impl MemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn with<T>(&self, f: impl FnOnce(&mut MemoryMandateStore) -> T) -> Result<T> {
        let mut store = self
            .0
            .lock()
            .map_err(|_| BackendError("memory store lock poisoned".into()))?;
        Ok(f(&mut store))
    }
}

impl Backend for MemoryBackend {
    fn name(&self) -> &'static str {
        "memory"
    }

    async fn mandates_by_ids(&self, ids: &[MandateId]) -> Result<Vec<Mandate>> {
        self.with(|s| ids.iter().flat_map(|id| s.get_mandates(id)).collect())
    }

    async fn mandates_for_subject(
        &self,
        subject: &PrincipalId,
        action: Option<&str>,
        resource: Option<&ResourceId>,
    ) -> Result<Vec<Mandate>> {
        self.with(|s| {
            s.mandates()
                .iter()
                .filter(|m| {
                    &m.subject == subject
                        && action.is_none_or(|a| m.action == a)
                        && resource.is_none_or(|r| &m.resource == r)
                })
                .cloned()
                .collect()
        })
    }

    async fn revocations_for(&self, mandate_ids: &[MandateId]) -> Result<Vec<Revocation>> {
        self.with(|s| {
            mandate_ids
                .iter()
                .flat_map(|id| s.find_revocations(id))
                .collect()
        })
    }

    async fn publish_mandate(&self, mandate: &Mandate) -> Result<Published> {
        self.with(|s| {
            let before = s.mandates().len();
            s.insert_mandate(mandate.clone());
            Published {
                created: s.mandates().len() > before,
                ..Published::default()
            }
        })
    }

    async fn publish_revocation(&self, revocation: &Revocation, _: u64) -> Result<Published> {
        self.with(|s| {
            let before = s.revocations().len();
            s.insert_revocation(revocation.clone());
            Published {
                created: s.revocations().len() > before,
                ..Published::default()
            }
        })
    }
}
