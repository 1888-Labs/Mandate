//! Demo mode for the public web UI, so judges can try Mandate without keys.
//!
//! Enabled by `DEMO_KEYS` (`name=0xkey,…`): the *synthetic* keys of the demo
//! principals. The server uses only FinanceAgent's key, to sign sandbox
//! delegations and their revocations; the others just give addresses names.
//!
//! The shared dataset (Alice → FinanceAgent → InvoiceAgent, LegacyBot) is
//! never modified from the UI. A "sandbox" is a fresh agent that FinanceAgent
//! delegates ≤ $50 to AWS for 2 hours; only sandbox mandates can be revoked
//! here. Writes are rate-limited, because each one spends testnet gas.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use mandate_core::{
    verify_chain, Constraints, Mandate, PrincipalId, ResourceId, TrustAnchors, UnsignedMandate,
    UnsignedRevocation,
};
use mandate_crypto::{
    address_of, generate_signing_key, sign_mandate, sign_revocation, signing_key_from_hex,
    EthereumVerifier, SigningKey,
};

use crate::backend::{Backend, BackendError, Published};
use crate::snapshot;

pub const SANDBOX_LIFETIME: u64 = 2 * 3600;
pub const SANDBOX_MAX_AMOUNT: u64 = 50;
/// Sandbox writes (delegations and revocations) allowed per hour, in total.
pub const WRITES_PER_HOUR: usize = 30;

pub struct Demo {
    pub resource: ResourceId,
    /// Display name → address, in configuration order.
    pub principals: Vec<(String, PrincipalId)>,
    finance: SigningKey,
    sandbox: Mutex<Sandbox>,
}

#[derive(Default)]
struct Sandbox {
    agents: BTreeMap<PrincipalId, String>,
    writes: VecDeque<u64>,
}

#[derive(Debug)]
pub enum DemoError {
    RateLimited,
    NoRootMandate,
    NotASandboxMandate,
    Signing(String),
    Backend(BackendError),
}

impl From<BackendError> for DemoError {
    fn from(e: BackendError) -> Self {
        Self::Backend(e)
    }
}

impl Demo {
    /// Parse `DEMO_KEYS`. It must include `finance-agent`.
    pub fn from_keys(spec: &str, resource: ResourceId) -> Result<Self, String> {
        let mut principals = Vec::new();
        let mut finance = None;
        for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (name, key) = entry
                .split_once('=')
                .ok_or_else(|| format!("`{entry}`: expected name=0xkey"))?;
            let key = signing_key_from_hex(key.trim()).map_err(|e| format!("{name}: {e}"))?;
            principals.push((name.trim().to_string(), address_of(&key)));
            if name.trim() == "finance-agent" {
                finance = Some(key);
            }
        }
        let finance = finance.ok_or("DEMO_KEYS must include finance-agent")?;
        Ok(Self {
            resource,
            principals,
            finance,
            sandbox: Mutex::new(Sandbox::default()),
        })
    }

    pub fn finance(&self) -> PrincipalId {
        address_of(&self.finance)
    }

    pub fn sandbox_agents(&self) -> Vec<(String, PrincipalId)> {
        self.sandbox
            .lock()
            .map(|s| {
                s.agents
                    .iter()
                    .map(|(a, n)| (n.clone(), a.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn take_write(&self, now: u64) -> Result<(), DemoError> {
        let mut s = self.sandbox.lock().map_err(|_| DemoError::RateLimited)?;
        while s
            .writes
            .front()
            .is_some_and(|t| now.saturating_sub(*t) >= 3600)
        {
            s.writes.pop_front();
        }
        if s.writes.len() >= WRITES_PER_HOUR {
            return Err(DemoError::RateLimited);
        }
        s.writes.push_back(now);
        Ok(())
    }

    /// FinanceAgent delegates ≤ $50 to AWS, for 2 hours, to a brand-new agent.
    pub async fn create_sandbox<B: Backend>(
        &self,
        backend: &B,
        anchors: &TrustAnchors,
        now: u64,
    ) -> Result<(String, Mandate, Published), DemoError> {
        // FinanceAgent's currently valid root grant from Alice.
        let finance = self.finance();
        let leaves = backend
            .mandates_for_subject(&finance, Some("transfer"), Some(&self.resource), Some(now))
            .await?;
        let snap = snapshot::for_leaves(backend, leaves).await?;
        let root = snap
            .store
            .mandates()
            .iter()
            .filter(|m| m.subject == finance && m.parent.is_none() && m.delegable)
            .filter(|m| {
                verify_chain((*m).clone(), &snap.store, &EthereumVerifier, anchors, now).is_ok()
            })
            .max_by_key(|m| m.expires_at)
            .cloned()
            .ok_or(DemoError::NoRootMandate)?;

        self.take_write(now)?;
        let agent = generate_signing_key();
        let agent_addr = address_of(&agent);
        let name = format!("sandbox-{}", &agent_addr.0[2..6]);
        let child = sign_mandate(
            &UnsignedMandate {
                issuer: finance,
                subject: agent_addr.clone(),
                action: root.action.clone(),
                resource: root.resource.clone(),
                constraints: Constraints {
                    max_amount: Some(SANDBOX_MAX_AMOUNT),
                    allowed_recipients: Some(vec!["AWS".into()]),
                },
                parent: Some(root.id.clone()),
                delegable: false,
                issued_at: now,
                expires_at: root.expires_at.min(now + SANDBOX_LIFETIME),
                nonce: now,
            },
            &self.finance,
        )
        .map_err(|e| DemoError::Signing(e.to_string()))?;
        let published = backend.publish_mandate(&child).await?;
        if let Ok(mut s) = self.sandbox.lock() {
            s.agents.insert(agent_addr, name.clone());
        }
        Ok((name, child, published))
    }

    /// FinanceAgent revokes one of the sandbox mandates it created.
    pub async fn revoke_sandbox<B: Backend>(
        &self,
        backend: &B,
        mandate: &Mandate,
        now: u64,
    ) -> Result<(mandate_core::Revocation, Published), DemoError> {
        let is_sandbox = self
            .sandbox
            .lock()
            .map(|s| s.agents.contains_key(&mandate.subject))
            .unwrap_or(false);
        if mandate.issuer != self.finance() || !is_sandbox {
            return Err(DemoError::NotASandboxMandate);
        }
        self.take_write(now)?;
        let revocation = sign_revocation(
            &UnsignedRevocation {
                mandate_id: mandate.id.clone(),
                issuer: self.finance(),
                revoked_at: now,
                reason: Some("sandbox_revoked".into()),
            },
            &self.finance,
        )
        .map_err(|e| DemoError::Signing(e.to_string()))?;
        let published = backend
            .publish_revocation(&revocation, mandate.expires_at)
            .await?;
        Ok((revocation, published))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_hex(n: u8) -> String {
        format!("0x{}{:02x}", "00".repeat(31), n)
    }

    #[test]
    fn parses_demo_keys() {
        let spec = format!("alice={},finance-agent={}", key_hex(1), key_hex(2));
        let demo = Demo::from_keys(&spec, "wallet:x".parse().unwrap()).unwrap();
        assert_eq!(demo.principals.len(), 2);
        assert_eq!(demo.principals[0].0, "alice");
        assert_eq!(
            demo.principals[0].1,
            PrincipalId("0x7e5f4552091a69125d5dfcb7b8c2659029395bdf".into())
        );
        assert_eq!(demo.finance(), demo.principals[1].1);
    }

    #[test]
    fn requires_finance_agent() {
        let spec = format!("alice={}", key_hex(1));
        assert!(Demo::from_keys(&spec, "wallet:x".parse().unwrap()).is_err());
        assert!(Demo::from_keys("finance-agent=nope", "wallet:x".parse().unwrap()).is_err());
    }

    #[test]
    fn writes_are_rate_limited_per_hour() {
        let spec = format!("finance-agent={}", key_hex(2));
        let demo = Demo::from_keys(&spec, "wallet:x".parse().unwrap()).unwrap();
        for _ in 0..WRITES_PER_HOUR {
            assert!(demo.take_write(1000).is_ok());
        }
        assert!(matches!(demo.take_write(1000), Err(DemoError::RateLimited)));
        assert!(demo.take_write(1000 + 3600).is_ok(), "the window slides");
    }
}
