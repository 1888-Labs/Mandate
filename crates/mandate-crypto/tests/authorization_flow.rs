//! Demo steps 1–5 from BUILD_SPEC.md with real secp256k1 signatures.
//! (Revocation, steps 6–7, arrives with Milestone 6.)

use mandate_core::{
    authorize, AuthorizationDecision, AuthorizationRequest, Constraints, Mandate, MandateId,
    MandateStore, PrincipalId, ResourceId, TrustAnchors, UnsignedMandate,
};
use mandate_crypto::{address_of, generate_signing_key, sign_mandate, EthereumVerifier};

struct VecStore(Vec<Mandate>);

impl MandateStore for VecStore {
    fn get_mandate(&self, id: &MandateId) -> Option<Mandate> {
        self.0.iter().find(|m| &m.id == id).cloned()
    }

    fn find_mandates(&self, s: &PrincipalId, a: &str, r: &ResourceId) -> Vec<Mandate> {
        self.0
            .iter()
            .filter(|m| &m.subject == s && m.action == a && &m.resource == r)
            .cloned()
            .collect()
    }
}

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

    let store = VecStore(vec![m1, m2.clone(), forged]);
    let anchors = TrustAnchors::new().trust(treasury.clone(), address_of(&alice));
    let ask = |amount: u64, recipient: &str| {
        authorize(
            &AuthorizationRequest {
                subject: address_of(&invoice),
                action: "transfer".into(),
                resource: treasury.clone(),
                amount: Some(amount),
                recipient: Some(recipient.into()),
                timestamp: now + 3600,
            },
            &store,
            &EthereumVerifier,
            &anchors,
        )
    };

    // 3. $200 → AWS: ALLOW via Alice → FinanceAgent → InvoiceAgent.
    match ask(200, "AWS") {
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
    match ask(700, "AWS") {
        AuthorizationDecision::Deny(d) => assert_eq!(d.reason.code(), "AMOUNT_EXCEEDED"),
        other => panic!("expected DENY, got {other:?}"),
    }

    // 5. $100 → RandomVendor: DENY RECIPIENT_NOT_ALLOWED.
    match ask(100, "RandomVendor") {
        AuthorizationDecision::Deny(d) => {
            assert_eq!(d.reason.code(), "RECIPIENT_NOT_ALLOWED")
        }
        other => panic!("expected DENY, got {other:?}"),
    }
}
