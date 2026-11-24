//! The Warden's persistent state: the owner key, mandates, revocations and
//! the decision log. Lives on the Sprite's disk, so it survives sleep.

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use mandate_core::{
    authorize, AuthorizationDecision, AuthorizationRequest, Constraints, Mandate, MandateId,
    MemoryMandateStore, PrincipalId, ResourceId, Revocation, TrustAnchors, UnsignedMandate,
    UnsignedRevocation,
};
use mandate_crypto::{
    address_of, generate_signing_key, sign_mandate, sign_revocation, signing_key_from_hex,
    signing_key_to_hex, verify_mandate_signature, EthereumVerifier, SigningKey,
};
use serde::{Deserialize, Serialize};

const KEPT_DECISIONS: usize = 500;

/// One authorization decision, as shown on the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRecord {
    pub at: u64,
    pub agent: String,
    pub method: String,
    pub path: String,
    pub action: String,
    pub resource: String,
    pub recipient: Option<String>,
    pub decision: String,
    pub reason: Option<String>,
    pub mandate_id: Option<String>,
    pub delegation_path: Vec<String>,
    pub upstream_status: Option<u16>,
    pub note: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Persisted {
    mandates: Vec<Mandate>,
    revocations: Vec<Revocation>,
    #[serde(default)]
    labels: BTreeMap<String, String>,
    #[serde(default)]
    good_checkpoint: Option<String>,
}

pub struct WardenState {
    dir: PathBuf,
    owner: SigningKey,
    store: MemoryMandateStore,
    labels: BTreeMap<String, String>,
    decisions: VecDeque<DecisionRecord>,
    nonce: u64,
    pub good_checkpoint: Option<String>,
}

/// What an admin asks the owner to sign.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantRequest {
    pub subject: String,
    #[serde(default)]
    pub label: Option<String>,
    pub action: String,
    pub resource: String,
    #[serde(default)]
    pub max_amount: Option<u64>,
    #[serde(default)]
    pub allowed_recipients: Option<Vec<String>>,
    #[serde(default)]
    pub delegable: bool,
    /// Seconds from now. Defaults to seven days.
    #[serde(default)]
    pub expires_in: Option<u64>,
}

impl WardenState {
    /// Load state from `dir`, creating an owner key on first run.
    pub fn open(dir: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&dir)?;
        let key_path = dir.join("owner.key");
        let owner = match fs::read_to_string(&key_path) {
            Ok(hex) => signing_key_from_hex(hex.trim())
                .map_err(|e| std::io::Error::other(format!("bad owner.key: {e:?}")))?,
            Err(_) => {
                let key = generate_signing_key();
                fs::write(&key_path, signing_key_to_hex(&key))?;
                restrict(&key_path);
                key
            }
        };
        let persisted: Persisted = fs::read_to_string(dir.join("store.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let mut store = MemoryMandateStore::new();
        let nonce = persisted.mandates.len() as u64;
        for m in persisted.mandates {
            store.insert_mandate(m);
        }
        for r in persisted.revocations {
            store.insert_revocation(r);
        }
        let decisions = fs::read_to_string(dir.join("decisions.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect::<VecDeque<_>>();
        let skip = decisions.len().saturating_sub(KEPT_DECISIONS);
        Ok(Self {
            dir,
            owner,
            store,
            labels: persisted.labels,
            decisions: decisions.into_iter().skip(skip).collect(),
            nonce,
            good_checkpoint: persisted.good_checkpoint,
        })
    }

    pub fn owner(&self) -> PrincipalId {
        address_of(&self.owner)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let persisted = Persisted {
            mandates: self.store.mandates().to_vec(),
            revocations: self.store.revocations().to_vec(),
            labels: self.labels.clone(),
            good_checkpoint: self.good_checkpoint.clone(),
        };
        let tmp = self.dir.join("store.json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&persisted)?)?;
        fs::rename(tmp, self.dir.join("store.json"))
    }

    /// The owner signs a root mandate for `req.subject`.
    pub fn grant(&mut self, req: GrantRequest, now: u64) -> Result<Mandate, String> {
        let resource: ResourceId = req.resource.parse().map_err(|e| format!("resource: {e}"))?;
        let subject =
            mandate_crypto::parse_address(&req.subject).map_err(|e| format!("subject: {e:?}"))?;
        self.nonce += 1;
        let unsigned = UnsignedMandate {
            issuer: self.owner(),
            subject: subject.clone(),
            action: req.action,
            resource,
            constraints: Constraints {
                max_amount: req.max_amount,
                allowed_recipients: req.allowed_recipients,
            },
            parent: None,
            delegable: req.delegable,
            issued_at: now,
            expires_at: now + req.expires_in.unwrap_or(7 * 24 * 3600),
            nonce: now * 1000 + self.nonce,
        };
        let mandate = sign_mandate(&unsigned, &self.owner).map_err(|e| format!("sign: {e:?}"))?;
        if let Some(label) = req.label {
            self.labels.insert(subject.0, label);
        }
        self.store.insert_mandate(mandate.clone());
        self.save().map_err(|e| e.to_string())?;
        Ok(mandate)
    }

    /// Accept a mandate signed by someone else (e.g. an agent delegating to
    /// a sub-agent). Only the signature is checked here; whether the chain
    /// grants anything is decided at authorization time.
    pub fn submit(&mut self, mandate: Mandate, label: Option<String>) -> Result<(), String> {
        verify_mandate_signature(&mandate).map_err(|e| format!("signature: {e:?}"))?;
        if let Some(label) = label {
            self.labels.insert(mandate.subject.0.clone(), label);
        }
        self.store.insert_mandate(mandate);
        self.save().map_err(|e| e.to_string())
    }

    /// The owner revokes one of its own mandates.
    pub fn revoke(
        &mut self,
        id: &str,
        reason: Option<String>,
        now: u64,
    ) -> Result<Revocation, String> {
        let mandate_id = MandateId(id.to_string());
        let target = self
            .store
            .mandates()
            .iter()
            .find(|m| m.id == mandate_id)
            .ok_or("unknown mandate")?;
        if target.issuer != self.owner() {
            return Err("only mandates issued by the owner can be revoked here".into());
        }
        let unsigned = UnsignedRevocation {
            mandate_id,
            issuer: self.owner(),
            revoked_at: now,
            reason,
        };
        let revocation =
            sign_revocation(&unsigned, &self.owner).map_err(|e| format!("sign: {e:?}"))?;
        self.store.insert_revocation(revocation.clone());
        self.save().map_err(|e| e.to_string())?;
        Ok(revocation)
    }

    pub fn authorize(&self, request: &AuthorizationRequest) -> AuthorizationDecision {
        // The owner is the only principal trusted to issue root mandates.
        let anchors = TrustAnchors::new().trust(request.resource.clone(), self.owner());
        authorize(request, &self.store, &EthereumVerifier, &anchors)
    }

    pub fn record(&mut self, record: DecisionRecord) {
        if let Ok(line) = serde_json::to_string(&record) {
            if let Ok(mut f) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.dir.join("decisions.jsonl"))
            {
                let _ = writeln!(f, "{line}");
            }
        }
        self.decisions.push_back(record);
        while self.decisions.len() > KEPT_DECISIONS {
            self.decisions.pop_front();
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "owner": self.owner(),
            "labels": self.labels,
            "mandates": self.store.mandates(),
            "revocations": self.store.revocations(),
            "decisions": self.decisions.iter().rev().take(200).collect::<Vec<_>>(),
            "goodCheckpoint": self.good_checkpoint,
        })
    }
}

#[cfg(unix)]
fn restrict(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict(_: &std::path::Path) {}
