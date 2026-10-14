# Protocol

[← Back to the README](../README.md)

How mandates, delegation and revocation work, precisely. The code is in [`crates/mandate-core`](../crates/mandate-core) and [`crates/mandate-crypto`](../crates/mandate-crypto).

## The core rule

> **A delegated mandate may only reduce authority. It may never expand it.**

Everything else in Mandate exists to enforce and explain this rule. It's implemented in a single function, [`can_delegate(parent, child)`](../crates/mandate-core/src/delegation.rs).

## Delegation

A child mandate is valid only if **all** of these hold:

| Rule | Rejected with |
|---|---|
| Parent is `delegable` | `PARENT_NOT_DELEGABLE` |
| Child's `parent` is the parent's ID | `INVALID_PARENT` |
| Child's `issuer` is the parent's `subject` | `INVALID_ISSUER` |
| Same `action` | `ACTION_ESCALATION` |
| Same `resource` | `RESOURCE_ESCALATION` |
| `max_amount` ≤ parent's, and kept if the parent had one | `AMOUNT_ESCALATION` |
| `allowed_recipients` ⊆ parent's, and kept if the parent had one | `RECIPIENT_ESCALATION` |
| `expires_at` ≤ parent's | `EXPIRATION_ESCALATION` |

A child can lower a limit, drop recipients or expire sooner. It can **never remove a restriction** its parent had: a parent capped at $1000 can't produce an uncapped child.

**Where authority starts.** A signature proves *who* granted authority, not that they *had* it to grant. So every chain must end at a root mandate issued by a **trust anchor**, a principal configured as an owner of that resource (in the demo, Alice owns `wallet:treasury-demo`). An agent that signs itself a root mandate gets `UNTRUSTED_ROOT`.

## Revocation

Revocation is a **separate signed document**, never an edit to the mandate. Only a mandate's own issuer can revoke it. In the running example, M1 is Alice's mandate to FinanceAgent and M2 is FinanceAgent's mandate to InvoiceAgent: Alice can revoke M1, and FinanceAgent can revoke M2.

While checking a request, the engine walks the whole chain from the agent's mandate up to the root. A valid revocation anywhere on that path denies the request:

| Revoked | InvoiceAgent's request is denied with |
|---|---|
| M2 (its own mandate) | `REVOKED` |
| M1 (an ancestor) | `ANCESTOR_REVOKED`, naming M1 and the revocation record |

A revocation counts only if it targets that exact mandate ID, comes from the mandate's issuer, and its signature verifies. Forged or misdirected revocations are ignored, and a forgery can't hide a real revocation: the engine checks every revocation the store returns, not just the first. A revocation applies as soon as it's visible. Its `revoked_at` timestamp is kept for audit only, so clock skew can't keep a revoked mandate alive.

## Mandate

| Field | Meaning |
|---|---|
| `id` | Deterministic ID, see below |
| `issuer` | Principal granting authority (lowercase EVM address) |
| `subject` | Principal receiving authority |
| `action` | Free-form verb: `transfer`, `read`, `merge_pr`, `send_email`, … |
| `resource` | `kind:id`, e.g. `wallet:treasury-demo`, `mcp:github:create_issue` |
| `constraints.max_amount` | Optional upper bound on amount |
| `constraints.allowed_recipients` | Optional set of allowed recipients |
| `parent` | ID of the mandate this was delegated from, or none for a root grant |
| `delegable` | Whether the subject may delegate further |
| `issued_at`, `expires_at` | Unix seconds |
| `nonce` | Lets an issuer create otherwise-identical mandates |
| `signature` | Issuer's 65-byte Ethereum signature |

Mandate is not specific to wallets. `action` and `resource` are plain strings, so the same model covers payments, GitHub, databases or MCP tools.

## Mandate IDs

```text
id = keccak256( canonical_encoding(mandate without id and signature) )
```

The canonical encoding is a hand-written, length-prefixed binary format with the domain separator `mandate/0.1` ([`encoding.rs`](../crates/mandate-core/src/encoding.rs)). It never depends on JSON field order. Changing any field that carries authority changes the ID, and the signature is never part of it. The recipient list is treated as a set, so order doesn't matter.

## Signatures

The issuer signs the 32-byte mandate digest with **EIP-191 `personal_sign`** (secp256k1), so any Ethereum wallet or tool can sign a mandate. A test pins a signature produced by Foundry's `cast` to make sure we interoperate.

Verification recomputes the digest from the mandate's content and never trusts the stored ID. It recovers the signer and requires it to equal `issuer`. Verification rejects:

- non-canonical addresses
- an ID that doesn't match the content
- malformed signatures
- malleable high-`s` signatures

## Decisions and deny codes

Every check returns **ALLOW** with evidence (`mandateId`, `rootPrincipal`, `delegationPath`, `chain`, `effectiveConstraints`) or **DENY** with one stable code and the evidence behind it:

| Code | Meaning |
|---|---|
| `NO_MANDATE` | The agent holds no mandate for this action and resource |
| `INVALID_SIGNATURE` | A mandate in the chain has a bad signature or a forged ID |
| `INVALID_DELEGATION` | A link escalates authority, a parent is missing, or the chain is longer than 16 hops |
| `UNTRUSTED_ROOT` | The chain doesn't start with a trusted owner of the resource |
| `EXPIRED` | A mandate in the chain has expired (the server's clock decides) |
| `REVOKED` | The agent's own mandate was revoked |
| `ANCESTOR_REVOKED` | A mandate above the agent's was revoked; names the highest one |
| `AMOUNT_EXCEEDED` | Over the effective limit, or no amount given where a limit applies |
| `RECIPIENT_NOT_ALLOWED` | Recipient not in the effective set, or none given where recipients are restricted |

If an agent holds several mandates and none authorizes the request, the denial comes from the one that got closest, so a planted forgery can't hide the real reason.
