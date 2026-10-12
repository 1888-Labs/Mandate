use std::fmt;

use serde::{Deserialize, Serialize};

use crate::capability::{Constraints, ResourceId};
use crate::principal::PrincipalId;

/// Deterministic identifier of a mandate (`0x`-prefixed keccak256 hex).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MandateId(pub String);

impl fmt::Display for MandateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A signed grant of authority from `issuer` to `subject`.
///
/// `action` is a free-form string (`transfer`, `read`, `merge_pr`, ...)
/// so the protocol is not tied to wallets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mandate {
    pub id: MandateId,

    pub issuer: PrincipalId,
    pub subject: PrincipalId,

    pub action: String,
    pub resource: ResourceId,

    pub constraints: Constraints,

    pub parent: Option<MandateId>,
    pub delegable: bool,

    pub issued_at: u64,
    pub expires_at: u64,

    pub nonce: u64,

    #[serde(with = "crate::hex_bytes")]
    pub signature: Vec<u8>,
}

impl Mandate {
    /// The authorization content of this mandate, without its ID or signature.
    pub fn unsigned(&self) -> UnsignedMandate {
        UnsignedMandate {
            issuer: self.issuer.clone(),
            subject: self.subject.clone(),
            action: self.action.clone(),
            resource: self.resource.clone(),
            constraints: self.constraints.clone(),
            parent: self.parent.clone(),
            delegable: self.delegable,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
            nonce: self.nonce,
        }
    }
}

/// Every field of a mandate that carries authority.
///
/// This is what the mandate ID is computed from and what the issuer signs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsignedMandate {
    pub issuer: PrincipalId,
    pub subject: PrincipalId,

    pub action: String,
    pub resource: ResourceId,

    pub constraints: Constraints,

    pub parent: Option<MandateId>,
    pub delegable: bool,

    pub issued_at: u64,
    pub expires_at: u64,

    pub nonce: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Mandate {
        Mandate {
            id: MandateId("0x01".to_string()),
            issuer: PrincipalId("0xA11CE".to_string()),
            subject: PrincipalId("0xF1NANCE".to_string()),
            action: "transfer".to_string(),
            resource: "wallet:treasury-demo".parse().unwrap(),
            constraints: Constraints {
                max_amount: Some(1000),
                allowed_recipients: Some(vec!["AWS".to_string()]),
            },
            parent: None,
            delegable: true,
            issued_at: 1_760_000_000,
            expires_at: 1_760_172_800,
            nonce: 1,
            signature: vec![0xde, 0xad, 0xbe, 0xef],
        }
    }

    #[test]
    fn mandate_json_matches_payload_shape() {
        let json = serde_json::to_value(sample()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "id": "0x01",
                "issuer": "0xA11CE",
                "subject": "0xF1NANCE",
                "action": "transfer",
                "resource": { "kind": "wallet", "id": "treasury-demo" },
                "constraints": { "maxAmount": 1000, "allowedRecipients": ["AWS"] },
                "parent": null,
                "delegable": true,
                "issuedAt": 1_760_000_000u64,
                "expiresAt": 1_760_172_800u64,
                "nonce": 1,
                "signature": "0xdeadbeef"
            })
        );
    }

    #[test]
    fn mandate_round_trips_through_json() {
        let mut m = sample();
        m.parent = Some(MandateId("0x00".to_string()));
        let json = serde_json::to_string(&m).unwrap();
        let back: Mandate = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn signature_without_0x_prefix_is_rejected() {
        let mut json = serde_json::to_value(sample()).unwrap();
        json["signature"] = serde_json::json!("deadbeef");
        assert!(serde_json::from_value::<Mandate>(json).is_err());
    }

    #[test]
    fn signature_with_invalid_hex_is_rejected() {
        let mut json = serde_json::to_value(sample()).unwrap();
        json["signature"] = serde_json::json!("0xzz");
        assert!(serde_json::from_value::<Mandate>(json).is_err());
    }

    #[test]
    fn equality_detects_any_field_change() {
        let base = sample();
        assert_eq!(base, sample());

        let mut m = sample();
        m.constraints.max_amount = Some(1001);
        assert_ne!(m, base);

        let mut m = sample();
        m.subject = PrincipalId("0xOTHER".to_string());
        assert_ne!(m, base);

        let mut m = sample();
        m.delegable = false;
        assert_ne!(m, base);

        let mut m = sample();
        m.signature = vec![0x00];
        assert_ne!(m, base);
    }
}
