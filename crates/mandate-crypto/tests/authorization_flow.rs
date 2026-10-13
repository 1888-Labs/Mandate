//! Milestone 7 acceptance: the full seven-step demo flow (README, "How it
//! works") with real secp256k1 signatures and `MemoryMandateStore`, no Arkiv.

use mandate_core::{
    authorize, AuthorizationDecision, AuthorizationRequest, Constraints, MemoryMandateStore,
    ResourceId, TrustAnchors, UnsignedMandate, UnsignedRevocation,
};
use mandate_crypto::{
    address_of, generate_signing_key, sign_mandate, sign_revocation, EthereumVerifier,
};

#[test]
fn demo_flow_with_real_signatures() {
    let now = 1_760_000_000;
    let treasury: ResourceId = "wallet:treasury-demo".parse().unwrap();

    let alice = generate_signing_key();
    let finance = generate_signing_key();
    let invoice = generate_signing_key();
    let mallory = generate_signing_key();

    // 1. Alice → FinanceAgent: ≤ $1000 to AWS, delegable, 48h.
    let m1 = sign_mandate(
        &UnsignedMandate {
            issuer: address_of(&alice),
            subject: address_of(&finance),
            action: "transfer".into(),
            resource: treasury.clone(),
            constraints: Constraints {
                max_amount: Some(1000),
                allowed_recipients: Some(vec!["AWS".into()]),
            },
            parent: None,
            delegable: true,
            issued_at: now,
            expires_at: now + 48 * 3600,
            nonce: 1,
        },
        &alice,
    )
    .unwrap();

    // 2. FinanceAgent → InvoiceAgent: ≤ $300 to AWS, 24h.
    let m2 = sign_mandate(
        &UnsignedMandate {
            issuer: address_of(&finance),
            subject: address_of(&invoice),
            constraints: Constraints {
                max_amount: Some(300),
                allowed_recipients: Some(vec!["AWS".into()]),
            },
            parent: Some(m1.id.clone()),
            delegable: false,
            expires_at: now + 24 * 3600,
            nonce: 2,
            ..m1.unsigned()
        },
        &finance,
    )
    .unwrap();

    // Mallory plants a mandate claiming to come from Alice.
    let mut forged = m1.clone();
    forged.subject = address_of(&invoice);
    forged.constraints = Constraints::default();
    let forged = forged.unsigned();
    let forged_sig = sign_mandate(
        &UnsignedMandate {
            issuer: address_of(&mallory),
            ..forged.clone()
        },
        &mallory,
    )
    .unwrap()
    .signature;
    let forged = forged.into_mandate(forged_sig);

    // Mallory also republishes M1 under its real ID with a bad signature.
    let mut broken_copy = m1.clone();
    broken_copy.signature = vec![0x00; 65];

    let mut store = MemoryMandateStore::new();
    store.insert_mandate(broken_copy);
    store.insert_mandate(m1.clone());
    store.insert_mandate(m2.clone());
    store.insert_mandate(forged);
    let anchors = TrustAnchors::new().trust(treasury.clone(), address_of(&alice));
    let ask = |store: &MemoryMandateStore, amount: u64, recipient: &str| {
        authorize(
            &AuthorizationRequest {
                subject: address_of(&invoice),
                action: "transfer".into(),
                resource: treasury.clone(),
                amount: Some(amount),
                recipient: Some(recipient.into()),
                timestamp: now + 3600,
            },
            store,
            &EthereumVerifier,
            &anchors,
        )
    };

    // 3. $200 → AWS: ALLOW via Alice → FinanceAgent → InvoiceAgent.
    match ask(&store, 200, "AWS") {
        AuthorizationDecision::Allow(e) => {
            assert_eq!(e.mandate_id, m2.id);
            assert_eq!(e.root_principal, address_of(&alice));
            assert_eq!(
                e.delegation_path,
                vec![
                    address_of(&alice),
                    address_of(&finance),
                    address_of(&invoice)
                ]
            );
        }
        AuthorizationDecision::Deny(d) => panic!("expected ALLOW, got {d:?}"),
    }

    // 4. $700 → AWS: DENY AMOUNT_EXCEEDED (the forgery does not help).
    match ask(&store, 700, "AWS") {
        AuthorizationDecision::Deny(d) => assert_eq!(d.reason.code(), "AMOUNT_EXCEEDED"),
        other => panic!("expected DENY, got {other:?}"),
    }

    // 5. $100 → RandomVendor: DENY RECIPIENT_NOT_ALLOWED.
    match ask(&store, 100, "RandomVendor") {
        AuthorizationDecision::Deny(d) => {
            assert_eq!(d.reason.code(), "RECIPIENT_NOT_ALLOWED")
        }
        other => panic!("expected DENY, got {other:?}"),
    }

    // Mallory publishes a revocation of M1 "from Alice". It must be ignored.
    let fake_revocation = sign_revocation(
        &UnsignedRevocation {
            mandate_id: m1.id.clone(),
            issuer: address_of(&mallory),
            revoked_at: now + 7200,
            reason: None,
        },
        &mallory,
    )
    .unwrap();
    store.insert_revocation(fake_revocation);
    assert!(ask(&store, 100, "AWS").is_allowed());

    // 6. Alice revokes M1.
    let revocation = sign_revocation(
        &UnsignedRevocation {
            mandate_id: m1.id.clone(),
            issuer: address_of(&alice),
            revoked_at: now + 7200,
            reason: Some("user_revoked".into()),
        },
        &alice,
    )
    .unwrap();
    store.insert_revocation(revocation.clone());

    // 7. $100 → AWS: DENY ANCESTOR_REVOKED, naming M1 and the full path.
    match ask(&store, 100, "AWS") {
        AuthorizationDecision::Deny(d) => {
            assert_eq!(d.reason.code(), "ANCESTOR_REVOKED");
            assert_eq!(
                d.reason,
                mandate_core::DenyReason::AncestorRevoked {
                    revoked: m1.id.clone(),
                    revocation_id: revocation.id.clone(),
                }
            );
            assert_eq!(d.mandate_id, Some(m2.id.clone()));
            assert_eq!(
                d.delegation_path,
                vec![
                    address_of(&alice),
                    address_of(&finance),
                    address_of(&invoice)
                ]
            );
        }
        other => panic!("expected DENY, got {other:?}"),
    }
}
