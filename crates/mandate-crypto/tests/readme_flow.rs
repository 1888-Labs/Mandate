//! The example from docs/library.md. Keep the two in sync.

use std::error::Error;

use mandate_core::{
    authorize, AuthorizationDecision, AuthorizationRequest, Constraints, DenyReason,
    MemoryMandateStore, PrincipalId, ResourceId, TrustAnchors, UnsignedMandate, UnsignedRevocation,
};
use mandate_crypto::{
    address_of, generate_signing_key, sign_mandate, sign_revocation, EthereumVerifier,
};

#[test]
fn library_example() -> Result<(), Box<dyn Error>> {
    let now = 1_795_000_000;
    let repo: ResourceId = "github:acme/website".parse()?;

    // Every principal is a secp256k1 key. Its identity is its EVM address.
    let owner = generate_signing_key();
    let agent = generate_signing_key();
    let helper = generate_signing_key();

    // The owner lets the agent write files in acme/website, but only on
    // branches under agent/, for one day. It may pass part of that on.
    let grant = sign_mandate(
        &UnsignedMandate {
            issuer: address_of(&owner),
            subject: address_of(&agent),
            action: "contents.write".into(),
            resource: repo.clone(),
            constraints: Constraints {
                max_amount: None,
                allowed_recipients: Some(vec!["agent/work".into(), "agent/docs".into()]),
            },
            parent: None,
            delegable: true,
            issued_at: now,
            expires_at: now + 24 * 3600,
            nonce: 1,
        },
        &owner,
    )?;

    // The agent hands a helper one of its two branches, for an hour.
    let sub = sign_mandate(
        &UnsignedMandate {
            issuer: address_of(&agent),
            subject: address_of(&helper),
            constraints: Constraints {
                allowed_recipients: Some(vec!["agent/docs".into()]),
                ..grant.constraints.clone()
            },
            parent: Some(grant.id.clone()),
            delegable: false,
            expires_at: now + 3600,
            nonce: 2,
            ..grant.unsigned()
        },
        &agent,
    )?;

    // Storage is untrusted: the engine re-verifies everything it reads.
    let mut store = MemoryMandateStore::new();
    store.insert_mandate(grant.clone());
    store.insert_mandate(sub.clone());

    // Only the owner may start a chain for this repository.
    let anchors = TrustAnchors::new().trust(repo.clone(), address_of(&owner));

    let write = |who: &PrincipalId, branch: &str| AuthorizationRequest {
        subject: who.clone(),
        action: "contents.write".into(),
        resource: repo.clone(),
        amount: None,
        recipient: Some(branch.into()),
        timestamp: now + 60,
    };

    // The helper writes to its branch: allowed, with the full chain as evidence.
    match authorize(
        &write(&address_of(&helper), "agent/docs"),
        &store,
        &EthereumVerifier,
        &anchors,
    ) {
        AuthorizationDecision::Allow(evidence) => {
            assert_eq!(evidence.root_principal, address_of(&owner));
            assert_eq!(evidence.chain, vec![grant.id.clone(), sub.id.clone()]);
        }
        AuthorizationDecision::Deny(denial) => panic!("unexpected deny: {}", denial.reason),
    }

    // The agent tries to push to main: denied, and the reason says why.
    let AuthorizationDecision::Deny(denial) = authorize(
        &write(&address_of(&agent), "main"),
        &store,
        &EthereumVerifier,
        &anchors,
    ) else {
        panic!("writes to main must be denied");
    };
    assert_eq!(denial.reason.code(), "RECIPIENT_NOT_ALLOWED");

    // The owner revokes the agent. Everything delegated beneath it falls too.
    store.insert_revocation(sign_revocation(
        &UnsignedRevocation {
            mandate_id: grant.id.clone(),
            issuer: address_of(&owner),
            revoked_at: now + 120,
            reason: Some("task finished".into()),
        },
        &owner,
    )?);

    let mut later = write(&address_of(&helper), "agent/docs");
    later.timestamp = now + 180;
    let AuthorizationDecision::Deny(denial) =
        authorize(&later, &store, &EthereumVerifier, &anchors)
    else {
        panic!("revocation must cascade");
    };
    assert!(matches!(denial.reason, DenyReason::AncestorRevoked { .. }));

    Ok(())
}
