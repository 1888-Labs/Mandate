//! A mock payment tool that honours Mandate.
//!
//! The authorization check lives here, in the tool, not in the agent: the
//! agent may be compromised. The tool knows which principal it is acting
//! for because it was set up with that identity (in real life, the
//! credential the agent uses to call the tool). Nothing the agent reads can
//! change it.

use mandate_core::PrincipalId;
use serde_json::{json, Value};

/// What happened to a requested transfer.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Executed { receipt: String },
    Blocked { reason: String, decision: Value },
}

pub struct PaymentTool {
    mandate_url: String,
    http: reqwest::Client,
    acting_for: PrincipalId,
    resource: (String, String),
    /// Transfers actually executed: (amount, recipient).
    pub ledger: Vec<(u64, String)>,
}

impl PaymentTool {
    pub fn new(mandate_url: &str, acting_for: PrincipalId, wallet: &str) -> Self {
        Self {
            mandate_url: mandate_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
            acting_for,
            resource: ("wallet".into(), wallet.into()),
            ledger: Vec::new(),
        }
    }

    /// Ask Mandate, then pay only on ALLOW. Any failure to get an answer
    /// counts as a denial.
    pub async fn transfer(&mut self, amount: u64, recipient: &str) -> (Outcome, Value) {
        let decision = match self.authorize(amount, recipient).await {
            Ok(d) => d,
            Err(e) => json!({ "decision": "ERROR", "reason": "MANDATE_UNREACHABLE", "message": e }),
        };
        if decision["decision"] == "ALLOW" {
            self.ledger.push((amount, recipient.to_string()));
            let receipt = format!("mock-tx-{:04}", self.ledger.len());
            (Outcome::Executed { receipt }, decision)
        } else {
            let reason = decision["reason"]
                .as_str()
                .or_else(|| decision["error"].as_str())
                .unwrap_or("UNKNOWN")
                .to_string();
            (
                Outcome::Blocked {
                    reason,
                    decision: decision.clone(),
                },
                decision,
            )
        }
    }

    async fn authorize(&self, amount: u64, recipient: &str) -> Result<Value, String> {
        let res = self
            .http
            .post(format!("{}/v1/authorize", self.mandate_url))
            .json(&json!({
                "subject": self.acting_for,
                "action": "transfer",
                "resource": { "kind": self.resource.0, "id": self.resource.1 },
                "amount": amount,
                "recipient": recipient,
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        res.json::<Value>().await.map_err(|e| e.to_string())
    }
}
