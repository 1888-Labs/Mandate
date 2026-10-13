//! Canonical binary encoding of an unsigned mandate, and mandate IDs.
//!
//! The encoding is explicit and does not depend on JSON or field ordering
//! of any serializer. Format (all integers big-endian):
//!
//! ```text
//! str          u64 byte length || UTF-8 bytes
//! u64          8 bytes
//! bool         1 byte: 0x00 | 0x01
//! Option<T>    0x00 (None) | 0x01 || T
//! list<str>    u64 count || str*   (sorted, duplicates removed)
//!
//! mandate =
//!     str("mandate/0.1")       domain separator
//!     str(issuer)
//!     str(subject)
//!     str(action)
//!     str(resource.kind)
//!     str(resource.id)
//!     Option<u64>(constraints.max_amount)
//!     Option<list<str>>(constraints.allowed_recipients)
//!     Option<str>(parent)
//!     bool(delegable)
//!     u64(issued_at)
//!     u64(expires_at)
//!     u64(nonce)
//! ```
//!
//! Every variable-length value is length-prefixed, so no two distinct
//! mandates can produce the same byte string.
//!
//! `allowed_recipients` is a set: its canonical form is sorted and
//! deduplicated, so `[AWS, GitHub]` and `[GitHub, AWS]` encode identically.
//!
//! Changing this format changes every mandate ID. Bump the domain
//! separator if that is ever done.

use sha3::{Digest, Keccak256};

use crate::mandate::{MandateId, UnsignedMandate};

/// Domain separator prepended to every encoded mandate.
pub const MANDATE_DOMAIN: &str = "mandate/0.1";

/// The exact bytes a mandate ID is computed from.
pub fn canonical_encoding(mandate: &UnsignedMandate) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);

    put_str(&mut out, MANDATE_DOMAIN);

    put_str(&mut out, &mandate.issuer.0);
    put_str(&mut out, &mandate.subject.0);

    put_str(&mut out, &mandate.action);
    put_str(&mut out, &mandate.resource.kind);
    put_str(&mut out, &mandate.resource.id);

    match mandate.constraints.max_amount {
        None => out.push(0),
        Some(amount) => {
            out.push(1);
            put_u64(&mut out, amount);
        }
    }

    match &mandate.constraints.allowed_recipients {
        None => out.push(0),
        Some(recipients) => {
            out.push(1);
            let mut set: Vec<&str> = recipients.iter().map(String::as_str).collect();
            set.sort_unstable();
            set.dedup();
            put_len(&mut out, set.len());
            for recipient in set {
                put_str(&mut out, recipient);
            }
        }
    }

    match &mandate.parent {
        None => out.push(0),
        Some(parent) => {
            out.push(1);
            put_str(&mut out, &parent.0);
        }
    }

    out.push(u8::from(mandate.delegable));

    put_u64(&mut out, mandate.issued_at);
    put_u64(&mut out, mandate.expires_at);
    put_u64(&mut out, mandate.nonce);

    out
}

/// `keccak256(canonical_encoding(mandate))`: the 32 bytes behind a mandate ID.
///
/// This is also the message the issuer signs.
pub fn mandate_digest(mandate: &UnsignedMandate) -> [u8; 32] {
    Keccak256::digest(canonical_encoding(mandate)).into()
}

/// `mandate_digest` as `0x`-prefixed lowercase hex.
///
/// The signature is never part of the ID.
pub fn compute_mandate_id(mandate: &UnsignedMandate) -> MandateId {
    MandateId(format!("0x{}", hex::encode(mandate_digest(mandate))))
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_len(out: &mut Vec<u8>, len: usize) {
    // usize is at most 64 bits on every supported target, so this is lossless.
    put_u64(out, len as u64);
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    put_len(out, value.len());
    out.extend_from_slice(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Constraints;
    use crate::principal::PrincipalId;

    fn unsigned() -> UnsignedMandate {
        UnsignedMandate {
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
        }
    }

    fn id_with(change: impl FnOnce(&mut UnsignedMandate)) -> MandateId {
        let mut m = unsigned();
        change(&mut m);
        compute_mandate_id(&m)
    }

    // --- Milestone 2 required tests ---

    #[test]
    fn same_mandate_has_same_id() {
        assert_eq!(
            compute_mandate_id(&unsigned()),
            compute_mandate_id(&unsigned())
        );
    }

    #[test]
    fn different_amount_has_different_id() {
        let base = compute_mandate_id(&unsigned());
        assert_ne!(id_with(|m| m.constraints.max_amount = Some(1001)), base);
    }

    #[test]
    fn different_subject_has_different_id() {
        let base = compute_mandate_id(&unsigned());
        assert_ne!(id_with(|m| m.subject = PrincipalId("0xOTHER".into())), base);
    }

    #[test]
    fn different_expiry_has_different_id() {
        let base = compute_mandate_id(&unsigned());
        assert_ne!(id_with(|m| m.expires_at += 1), base);
    }

    #[test]
    fn different_resource_has_different_id() {
        let base = compute_mandate_id(&unsigned());
        assert_ne!(
            id_with(|m| m.resource = "wallet:another-wallet".parse().unwrap()),
            base
        );
        assert_ne!(
            id_with(|m| m.resource = "database:treasury-demo".parse().unwrap()),
            base
        );
    }

    #[test]
    fn signature_not_part_of_mandate_id() {
        let a = unsigned().into_mandate(vec![0x01; 65]);
        let b = unsigned().into_mandate(vec![0x02; 65]);
        assert_ne!(a.signature, b.signature);
        assert_eq!(
            compute_mandate_id(&a.unsigned()),
            compute_mandate_id(&b.unsigned())
        );
        assert_eq!(a.id, b.id);
    }

    // --- Every authority-carrying field must affect the ID ---

    #[test]
    fn every_field_changes_id() {
        let base = compute_mandate_id(&unsigned());
        let variants = [
            id_with(|m| m.issuer = PrincipalId("0xB0B".into())),
            id_with(|m| m.subject = PrincipalId("0xB0B".into())),
            id_with(|m| m.action = "read".into()),
            id_with(|m| m.resource.kind = "api".into()),
            id_with(|m| m.resource.id = "other".into()),
            id_with(|m| m.constraints.max_amount = None),
            id_with(|m| m.constraints.max_amount = Some(999)),
            id_with(|m| m.constraints.allowed_recipients = None),
            id_with(|m| m.constraints.allowed_recipients = Some(vec!["Coinbase".into()])),
            id_with(|m| m.parent = Some(MandateId("0x00".into()))),
            id_with(|m| m.delegable = false),
            id_with(|m| m.issued_at += 1),
            id_with(|m| m.expires_at += 1),
            id_with(|m| m.nonce += 1),
        ];
        for (i, id) in variants.iter().enumerate() {
            assert_ne!(*id, base, "variant {i} did not change the ID");
        }
        // All variants are also distinct from each other.
        for (i, a) in variants.iter().enumerate() {
            for b in &variants[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    // --- Encoding is unambiguous ---

    #[test]
    fn field_boundaries_cannot_be_shifted() {
        // "ab" + "c" vs "a" + "bc" must not collide.
        let a = id_with(|m| {
            m.resource.kind = "ab".into();
            m.resource.id = "c".into();
        });
        let b = id_with(|m| {
            m.resource.kind = "a".into();
            m.resource.id = "bc".into();
        });
        assert_ne!(a, b);
    }

    #[test]
    fn absent_and_zero_amount_differ() {
        assert_ne!(
            id_with(|m| m.constraints.max_amount = None),
            id_with(|m| m.constraints.max_amount = Some(0)),
        );
    }

    #[test]
    fn unrestricted_and_empty_recipients_differ() {
        assert_ne!(
            id_with(|m| m.constraints.allowed_recipients = None),
            id_with(|m| m.constraints.allowed_recipients = Some(vec![])),
        );
    }

    #[test]
    fn recipients_are_a_set() {
        let ordered = id_with(|m| {
            m.constraints.allowed_recipients = Some(vec!["AWS".into(), "GitHub".into()])
        });
        let reversed = id_with(|m| {
            m.constraints.allowed_recipients = Some(vec!["GitHub".into(), "AWS".into()])
        });
        let duplicated = id_with(|m| {
            m.constraints.allowed_recipients =
                Some(vec!["AWS".into(), "GitHub".into(), "AWS".into()])
        });
        assert_eq!(ordered, reversed);
        assert_eq!(ordered, duplicated);
    }

    // --- Format stability ---

    #[test]
    fn id_is_0x_prefixed_keccak256_hex() {
        let id = compute_mandate_id(&unsigned());
        assert!(id.0.starts_with("0x"));
        assert_eq!(id.0.len(), 2 + 64);
        assert!(id.0[2..]
            .chars()
            .all(|c| matches!(c, '0'..='9' | 'a'..='f')));
    }

    #[test]
    fn canonical_encoding_is_pinned() {
        // Guards against accidental format changes. If this fails, the
        // encoding changed and every existing mandate ID is invalidated.
        let expected = concat!(
            "000000000000000b",
            "6d616e646174652f302e31", // "mandate/0.1"
            "0000000000000007",
            "30784131314345", // issuer "0xA11CE"
            "0000000000000009",
            "307846314e414e4345", // subject "0xF1NANCE"
            "0000000000000008",
            "7472616e73666572", // action "transfer"
            "0000000000000006",
            "77616c6c6574", // kind "wallet"
            "000000000000000d",
            "74726561737572792d64656d6f", // id "treasury-demo"
            "01",
            "00000000000003e8", // max_amount = Some(1000)
            "01",
            "0000000000000001", // recipients = Some(1 entry)
            "0000000000000003",
            "415753",           //   "AWS"
            "00",               // parent = None
            "01",               // delegable = true
            "0000000068e77800", // issued_at 1_760_000_000
            "0000000068ea1b00", // expires_at 1_760_172_800
            "0000000000000001", // nonce
        );
        assert_eq!(hex::encode(canonical_encoding(&unsigned())), expected);
    }

    #[test]
    fn keccak_matches_known_vector() {
        // keccak256("") — confirms we use Keccak-256, not NIST SHA3-256.
        assert_eq!(
            hex::encode(Keccak256::digest(b"")),
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }
}
