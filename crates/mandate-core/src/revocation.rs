//! Revocation documents.
//!
//! A revocation is a separate signed record. A mandate is never edited to
//! mark it revoked. Revoking a mandate also cuts off every mandate
//! delegated beneath it, with no further revocations needed.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::encoding::compute_revocation_id;
use crate::mandate::MandateId;
use crate::principal::PrincipalId;

/// Deterministic identifier of a revocation (`0x`-prefixed keccak256 hex).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RevocationId(pub String);

impl fmt::Display for RevocationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A signed statement that `mandate_id` no longer grants authority.
///
/// In v0.1 only the mandate's own issuer may revoke it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revocation {
    pub id: RevocationId,
    pub mandate_id: MandateId,
    pub issuer: PrincipalId,
    /// Unix seconds. Informational: a valid revocation applies as soon as
    /// it is published, whatever this says.
    pub revoked_at: u64,
    pub reason: Option<String>,
    #[serde(with = "crate::hex_bytes")]
    pub signature: Vec<u8>,
}

impl Revocation {
    /// The signed content of this revocation, without its ID or signature.
    pub fn unsigned(&self) -> UnsignedRevocation {
        UnsignedRevocation {
            mandate_id: self.mandate_id.clone(),
            issuer: self.issuer.clone(),
            revoked_at: self.revoked_at,
            reason: self.reason.clone(),
        }
    }
}

/// Everything a revocation's issuer signs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsignedRevocation {
    pub mandate_id: MandateId,
    pub issuer: PrincipalId,
    pub revoked_at: u64,
    pub reason: Option<String>,
}

impl UnsignedRevocation {
    /// Attach a signature, computing the revocation ID from this content.
    ///
    /// This does not check the signature. Use `mandate-crypto` for that.
    pub fn into_revocation(self, signature: Vec<u8>) -> Revocation {
        Revocation {
            id: compute_revocation_id(&self),
            mandate_id: self.mandate_id,
            issuer: self.issuer,
            revoked_at: self.revoked_at,
            reason: self.reason,
            signature,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::canonical_revocation_encoding;

    fn unsigned() -> UnsignedRevocation {
        UnsignedRevocation {
            mandate_id: MandateId("0x01".into()),
            issuer: PrincipalId("0xa11ce".into()),
            revoked_at: 1_760_003_600,
            reason: Some("user_revoked".into()),
        }
    }

    fn id_with(change: impl FnOnce(&mut UnsignedRevocation)) -> RevocationId {
        let mut r = unsigned();
        change(&mut r);
        compute_revocation_id(&r)
    }

    #[test]
    fn same_revocation_has_same_id() {
        assert_eq!(
            compute_revocation_id(&unsigned()),
            compute_revocation_id(&unsigned())
        );
    }

    #[test]
    fn every_field_changes_revocation_id() {
        let base = compute_revocation_id(&unsigned());
        let variants = [
            id_with(|r| r.mandate_id = MandateId("0x02".into())),
            id_with(|r| r.issuer = PrincipalId("0xb0b".into())),
            id_with(|r| r.revoked_at += 1),
            id_with(|r| r.reason = None),
            id_with(|r| r.reason = Some("compromised".into())),
        ];
        for (i, id) in variants.iter().enumerate() {
            assert_ne!(*id, base, "variant {i} did not change the ID");
        }
    }

    #[test]
    fn signature_not_part_of_revocation_id() {
        let a = unsigned().into_revocation(vec![1; 65]);
        let b = unsigned().into_revocation(vec![2; 65]);
        assert_eq!(a.id, b.id);
        assert_eq!(a.unsigned(), b.unsigned());
    }

    #[test]
    fn revocation_encoding_is_pinned() {
        let expected = concat!(
            "0000000000000016",
            "6d616e646174652d7265766f636174696f6e2f302e31", // "mandate-revocation/0.1"
            "0000000000000004",
            "30783031", // "0x01"
            "0000000000000007",
            "30786131316365",   // "0xa11ce"
            "0000000068e78610", // revoked_at 1_760_003_600
            "01",
            "000000000000000c",
            "757365725f7265766f6b6564", // "user_revoked"
        );
        assert_eq!(
            hex::encode(canonical_revocation_encoding(&unsigned())),
            expected
        );
    }

    #[test]
    fn revocation_json_matches_payload_shape() {
        let r = unsigned().into_revocation(vec![0xde, 0xad]);
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["mandateId"], "0x01");
        assert_eq!(json["issuer"], "0xa11ce");
        assert_eq!(json["revokedAt"], 1_760_003_600u64);
        assert_eq!(json["reason"], "user_revoked");
        assert_eq!(json["signature"], "0xdead");
        let back: Revocation = serde_json::from_value(json).unwrap();
        assert_eq!(back, r);
    }
}
