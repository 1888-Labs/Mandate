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

/// Where a document lives on Arkiv, and who put it there.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArkivRef {
    pub entity_key: Option<String>,
    /// `$creator`: the wallet that created the entity. Immutable on Arkiv.
    pub creator: Option<PrincipalId>,
    /// `$owner`: the wallet that can still change or delete the entity. The
    /// burn address once ownership is given up, as for revocations.
    pub owner: Option<PrincipalId>,
}

/// A document as read back from the backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored<T> {
    pub doc: T,
    pub arkiv: ArkivRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// The backend could not be reached or misbehaved. Authorization must
    /// fail closed on this, never allow.
    Unavailable(String),
    /// The backend refused a write, e.g. it can't publish as this issuer.
    Rejected { code: String, message: String },
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(m) => f.write_str(m),
            Self::Rejected { code, message } => write!(f, "{code}: {message}"),
        }
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
    ) -> impl Future<Output = Result<Vec<Stored<Mandate>>>> + Send;

    /// A subject's mandates; with `active_at`, only those expiring later.
    fn mandates_for_subject(
        &self,
        subject: &PrincipalId,
        action: Option<&str>,
        resource: Option<&ResourceId>,
        active_at: Option<u64>,
    ) -> impl Future<Output = Result<Vec<Stored<Mandate>>>> + Send;

    /// Revocations of each mandate *published by that mandate's issuer*
    /// (on Arkiv: `$creator` = issuer).
    fn revocations_for(
        &self,
        targets: &[(MandateId, PrincipalId)],
    ) -> impl Future<Output = Result<Vec<Stored<Revocation>>>> + Send;

    fn publish_mandate(&self, mandate: &Mandate) -> impl Future<Output = Result<Published>> + Send;

    /// Store a revocation, keeping it at least until `retain_until`.
    fn publish_revocation(
        &self,
        revocation: &Revocation,
        retain_until: u64,
    ) -> impl Future<Output = Result<Published>> + Send;
}

/// In-process state, for tests and running without Arkiv. Every document
/// counts as published by its own issuer.
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
            .map_err(|_| BackendError::Unavailable("memory store lock poisoned".into()))?;
        Ok(f(&mut store))
    }
}

fn by_issuer<T>(doc: T, issuer: &PrincipalId) -> Stored<T> {
    Stored {
        doc,
        arkiv: ArkivRef {
            entity_key: None,
            creator: Some(issuer.clone()),
            owner: None,
        },
    }
}

impl Backend for MemoryBackend {
    fn name(&self) -> &'static str {
        "memory"
    }

    async fn mandates_by_ids(&self, ids: &[MandateId]) -> Result<Vec<Stored<Mandate>>> {
        self.with(|s| {
            ids.iter()
                .flat_map(|id| s.get_mandates(id))
                .map(|m| {
                    let issuer = m.issuer.clone();
                    by_issuer(m, &issuer)
                })
                .collect()
        })
    }

    async fn mandates_for_subject(
        &self,
        subject: &PrincipalId,
        action: Option<&str>,
        resource: Option<&ResourceId>,
        active_at: Option<u64>,
    ) -> Result<Vec<Stored<Mandate>>> {
        self.with(|s| {
            s.mandates()
                .iter()
                .filter(|m| {
                    &m.subject == subject
                        && action.is_none_or(|a| m.action == a)
                        && resource.is_none_or(|r| &m.resource == r)
                        && active_at.is_none_or(|t| m.expires_at > t)
                })
                .map(|m| by_issuer(m.clone(), &m.issuer))
                .collect()
        })
    }

    async fn revocations_for(
        &self,
        targets: &[(MandateId, PrincipalId)],
    ) -> Result<Vec<Stored<Revocation>>> {
        self.with(|s| {
            targets
                .iter()
                .flat_map(|(id, issuer)| {
                    s.find_revocations(id)
                        .into_iter()
                        .filter(move |r| &r.issuer == issuer)
                })
                .map(|r| {
                    let issuer = r.issuer.clone();
                    by_issuer(r, &issuer)
                })
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
