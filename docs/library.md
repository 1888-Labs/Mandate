# Using Mandate as a library

[← Back to the README](../README.md)

The same flow is available from Rust. This example is compiled and run as a test ([`readme_flow.rs`](../crates/mandate-crypto/tests/readme_flow.rs)):

```rust
use mandate_core::{can_delegate, Constraints, DelegationError, UnsignedMandate};
use mandate_crypto::{address_of, generate_signing_key, sign_mandate, verify_mandate_signature};

let now = 1_760_000_000;

// Each principal is a secp256k1 key; its identity is the EVM address.
let alice = generate_signing_key();
let finance_agent = generate_signing_key();
let invoice_agent = generate_signing_key();

// M1 — Alice lets FinanceAgent transfer up to $1000 to AWS, and delegate.
let m1 = sign_mandate(
    &UnsignedMandate {
        issuer: address_of(&alice),
        subject: address_of(&finance_agent),
        action: "transfer".into(),
        resource: "wallet:treasury-demo".parse()?,
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
)?;

// M2 — FinanceAgent hands InvoiceAgent a narrower slice: $300, 24h.
let m2 = sign_mandate(
    &UnsignedMandate {
        issuer: address_of(&finance_agent),
        subject: address_of(&invoice_agent),
        constraints: Constraints {
            max_amount: Some(300),
            ..m1.constraints.clone()
        },
        parent: Some(m1.id.clone()),
        delegable: false,
        expires_at: now + 24 * 3600,
        nonce: 2,
        ..m1.unsigned()
    },
    &finance_agent,
)?;

verify_mandate_signature(&m1)?;
verify_mandate_signature(&m2)?;
can_delegate(&m1, &m2)?; // Ok: M2 only narrows M1.

// FinanceAgent tries to give InvoiceAgent more than it has.
let escalated = sign_mandate(
    &UnsignedMandate {
        constraints: Constraints {
            max_amount: Some(5000),
            ..m2.constraints.clone()
        },
        ..m2.unsigned()
    },
    &finance_agent,
)?;
assert_eq!(
    can_delegate(&m1, &escalated),
    Err(DelegationError::AmountEscalation)
);
```

The engine itself is `mandate_core::authorize(request, store, verifier, trust_anchors)`. Plug in any storage by implementing the `MandateStore` trait ([`store.rs`](../crates/mandate-core/src/store.rs)), or use `MemoryMandateStore`. `mandate_crypto::EthereumVerifier` is the signature verifier. See [protocol](protocol.md) for what each check means.
