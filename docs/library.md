# Using Mandate as a library

[← Back to the README](../README.md)

The Warden is one program built on the Mandate engine. The engine itself is a pair of Rust crates you can use in your own gateway, agent framework or service: anywhere you need to decide whether an agent may do something, and explain the answer.

| Crate | What it gives you |
|---|---|
| `mandate-core` | Mandates, revocations, delegation rules and the `authorize` engine. No I/O, no crypto, no async |
| `mandate-crypto` | secp256k1 keys, EIP-191 signing and `EthereumVerifier`, the signature checker the engine uses |
| `mandate-warden` | The Warden gateway and its tools. You only need it to run the Warden, not to embed the engine |

## Install

The crates are not on crates.io yet (see the [roadmap](roadmap.md)). Depend on them from Git:

```toml
[dependencies]
mandate-core = { git = "https://github.com/1888-Labs/Mandate" }
mandate-crypto = { git = "https://github.com/1888-Labs/Mandate" }
```

## Example: a branch-scoped coding agent

This is the same policy the Warden enforces on Sprites. An owner lets an agent write to a repository, but only on its own branches. The agent hands a helper a narrower slice, a write to `main` is refused with a reason, and revoking the agent cuts off the helper too.

The example is compiled and run as a test ([`readme_flow.rs`](../crates/mandate-crypto/tests/readme_flow.rs)):

```rust
use mandate_core::{
    authorize, AuthorizationDecision, AuthorizationRequest, Constraints, DenyReason,
    MemoryMandateStore, PrincipalId, ResourceId, TrustAnchors, UnsignedMandate, UnsignedRevocation,
};
use mandate_crypto::{
    address_of, generate_signing_key, sign_mandate, sign_revocation, EthereumVerifier,
};

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
```

## How the pieces fit

- **Request.** An `AuthorizationRequest` is who (`subject`), what (`action`), where (`resource`), plus an optional `amount` and `recipient`, at a `timestamp`. The Warden turns each HTTP call into one of these. For GitHub, the recipient is the branch, so `allowed_recipients` limits where an agent can write. See [how calls are classified](warden.md#how-calls-are-classified) for the full mapping.
- **Decision.** `authorize` returns `Allow` with the evidence (root owner, delegation path, mandate chain, effective limits) or `Deny` with a `DenyReason`. `reason.code()` gives the stable codes the Warden logs, such as `RECIPIENT_NOT_ALLOWED` or `ANCESTOR_REVOKED`. See [deny codes](protocol.md#decisions-and-deny-codes).
- **Trust anchors.** `TrustAnchors` says who may start a chain for each resource. Without it, any key could sign itself a root mandate.
- **Delegation.** A child mandate may only narrow its parent: same action and resource, fewer recipients, a lower amount, an earlier expiry. `can_delegate(parent, child)` checks one edge on its own; `authorize` checks every edge in the chain.
- **Revocation.** Only a mandate's issuer can revoke it, and revoking a mandate revokes everything delegated beneath it.

## Bring your own storage

`MemoryMandateStore` keeps everything in memory, which is what the Warden uses, saved to `store.json` in its data directory. To keep mandates somewhere else, implement the `MandateStore` trait ([`store.rs`](../crates/mandate-core/src/store.rs)):

```rust
pub trait MandateStore {
    fn get_mandates(&self, id: &MandateId) -> Vec<Mandate>;
    fn find_mandates(&self, subject: &PrincipalId, action: &str, resource: &ResourceId) -> Vec<Mandate>;
    fn find_revocations(&self, mandate_id: &MandateId) -> Vec<Revocation>;
}
```

Storage is untrusted. Return every record that matches, including duplicates: the engine re-verifies each signature and picks the copy that checks out, so a forged record can never hide a real one.

To use a different signature scheme, implement `SignatureVerifier` in place of `EthereumVerifier`.

See the [protocol](protocol.md) for exactly what each check means.
