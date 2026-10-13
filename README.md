# Mandate

**Verifiable delegated authorization for autonomous AI agents.**

Mandate lets a human give an AI agent narrowly scoped, cryptographically signed authority. That agent can hand a *smaller* piece of it to another agent, and anyone in the chain can revoke what they granted. Any action can be checked with an answer that explains itself: **ALLOW** or **DENY**, with the reason and the chain of authority behind it.

Mandates and revocations are stored as public, queryable, time-scoped records on [Arkiv](https://arkiv.network). A Rust engine verifies every signature and every delegation step itself, so the storage layer can never invent authority.

> **The prompt is not the security boundary. Mandate is.**

> [!NOTE]
> **Status: v0.1 in active development** for the Arkiv Global Tour Stop hackathon (Security track, submission 18 Oct 2026).
> Working and tested: the protocol core (signed mandates, delegation, revocation, the authorization engine) and a CLI that runs the full demo locally.
> The full demo runs against Arkiv Tiramisu through the Rust HTTP API (`scripts/demo-arkiv.sh`). The CLI signs locally and still keeps its own state in a local file; the demo publishes the CLI's signed documents through the API. See [Roadmap](#roadmap).

---

## Contents

- [The problem](#the-problem)
- [How it works](#how-it-works)
- [The core rule](#the-core-rule)
- [Delegation](#delegation)
- [Revocation](#revocation)
- [Why Arkiv](#why-arkiv)
- [Architecture](#architecture)
- [Quick start](#quick-start)
- [Protocol reference](#protocol-reference)
- [Security model](#security-model)
- [Threat model](#threat-model)
- [Inspiration and related work](#inspiration-and-related-work)
- [Roadmap](#roadmap)
- [Repository layout](#repository-layout)
- [Development](#development)
- [Hackathon disclosure](#hackathon-disclosure)
- [License](#license)

---

## The problem

AI agents now call MCP servers, APIs, wallets, cloud infrastructure, databases and payment systems, and they increasingly delegate work to *other agents*:

```text
Human ──delegates──▶ Agent A ──delegates part──▶ Agent B ──acts──▶ External system
```

Most authorization systems assume a human or app holding fairly static permissions. They can't answer the questions agent chains raise:

> Was this agent actually authorized to do this? Who granted that authority, under what limits, and has it since been revoked?

A system prompt that says "never spend more than $300" is not an answer. Prompt injection, bugs and confused agents can all talk past it. Mandate moves the limit out of the prompt and into signed, checkable authority.

## How it works

```text
Alice
  │  M1: transfer ≤ $1000 → AWS, may delegate, 48h
  ▼
FinanceAgent
  │  M2: transfer ≤ $300 → AWS, 24h
  ▼
InvoiceAgent
```

| InvoiceAgent requests | Decision | Why |
|---|---|---|
| $200 → AWS | ✅ ALLOW | Within M2, and M2 is a valid narrowing of M1 |
| $700 → AWS | ❌ DENY `AMOUNT_EXCEEDED` | Effective limit is $300 |
| $100 → RandomVendor | ❌ DENY `RECIPIENT_NOT_ALLOWED` | Only AWS is allowed |
| *Alice revokes M1*, then $100 → AWS | ❌ DENY `ANCESTOR_REVOKED` | M2's authority came from M1 |

InvoiceAgent's mandate never had to be revoked on its own. Revoking an ancestor cuts off everything delegated beneath it.

## The core rule

> **A delegated mandate may only reduce authority. It may never expand it.**

Everything else in Mandate exists to enforce and explain this rule. It's implemented in a single function, [`can_delegate(parent, child)`](crates/mandate-core/src/delegation.rs).

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

Revocation is a **separate signed document**, never an edit to the mandate. Only a mandate's own issuer can revoke it. Alice revokes M1; FinanceAgent revokes M2.

While checking a request, the engine walks the whole chain from the agent's mandate up to the root. A valid revocation anywhere on that path denies the request:

| Revoked | InvoiceAgent's request is denied with |
|---|---|
| M2 (its own mandate) | `REVOKED` |
| M1 (an ancestor) | `ANCESTOR_REVOKED`, naming M1 and the revocation record |

A revocation counts only if it targets that exact mandate ID, comes from the mandate's issuer, and its signature verifies. Forged or misdirected revocations are ignored, and a forgery can't hide a real revocation: the engine checks every revocation the store returns, not just the first. A revocation applies as soon as it's visible. Its `revoked_at` timestamp is kept for audit only, so clock skew can't keep a revoked mandate alive.

## Why Arkiv

Mandate needs a **shared, queryable, time-scoped** record of who has been granted what, and of what has been revoked, that no single operator controls.

| Alternative | Why not |
|---|---|
| Postgres | Every verifier must trust the operator, who could rewrite grants, hide revocations or show different consumers different answers. |
| IPFS | Content-addressed, but not a live queryable dataset. You can't ask "active mandates for agent X on resource Y". |
| Subgraph | Indexes data that lives somewhere else, so we'd first have to invent a contract and events to index. |

On Arkiv, each mandate is an entity whose lifetime tracks the mandate's expiry, with queryable attributes (subject, action, resource, parent). Revocations are entities too. The Rust engine still checks every signature and every expiry itself: Arkiv provides **availability and discoverability**, never **authority**.

**What never goes on Arkiv:** private keys, seed phrases, API secrets, prompts, model reasoning, private tool arguments or personal data. Only signed mandates, signed revocations and public authorization metadata.

## Architecture

```text
                     AI agent
                        │ "may I transfer $200 to AWS?"
                        ▼
               ┌──────────────────┐
               │  Mandate server  │  Rust: all authorization semantics
               └────────┬─────────┘
       ┌────────────────┼────────────────┐
       ▼                ▼                ▼
   signature       delegation        constraint
  verification    chain checks       evaluation
       └────────────────┼────────────────┘
                        ▼
              ALLOW / DENY + explanation
                        │ reads / writes
                        ▼
               ┌──────────────────┐
               │  Arkiv adapter   │  TypeScript, only because the Arkiv SDK is TS
               └────────┬─────────┘
                        ▼
              Arkiv (Tiramisu testnet)
```

Rust owns what authorization *means*. TypeScript only moves records to and from Arkiv.

## Quick start

Requirements: a recent stable Rust toolchain.

```bash
git clone <this repository>
cd mandate
cargo test
cargo install --path crates/mandate-cli   # installs the `mandate` binary
```

### CLI demo

The whole demo runs locally. State lives in `./.mandate/` (override with `--home` or `MANDATE_HOME`). This exact flow is an automated test ([`cli_demo.rs`](crates/mandate-cli/tests/cli_demo.rs)).

```bash
mandate principal create alice
mandate principal create finance-agent
mandate principal create invoice-agent

# Alice owns the treasury wallet, so only she can start a chain for it.
mandate resource own wallet:treasury-demo --owner alice

mandate grant --from alice --to finance-agent \
  --action transfer --resource wallet:treasury-demo \
  --max-amount 1000 --recipient AWS --delegable --expires-in 48h

mandate delegate --mandate M1 --from finance-agent --to invoice-agent \
  --max-amount 300 --expires-in 24h
```

```text
Delegation valid. Mandate M2 created

  alice
   └── finance-agent  [≤ $1000 → AWS]
        └── invoice-agent  [≤ $300 → AWS]
```

```bash
mandate check --agent invoice-agent --action transfer \
  --resource wallet:treasury-demo --amount 200 --recipient AWS
```

```text
✓ AUTHORIZED

  Agent        invoice-agent
  Request      transfer $200 → AWS  on wallet:treasury-demo
  Authority    alice
  Delegation   alice → finance-agent → invoice-agent
  Limits       ≤ $300 → AWS
  Mandate      M2
```

The same check with `--amount 700` gives `✗ DENIED  AMOUNT_EXCEEDED` (requested $700, maximum $300), and `--recipient RandomVendor` gives `✗ DENIED  RECIPIENT_NOT_ALLOWED`. Trying to delegate *more* than you hold is refused before anything is stored:

```text
✗ DELEGATION REJECTED  AMOUNT_ESCALATION

  child amount limit exceeds the parent's.
```

Now Alice revokes her grant:

```bash
mandate revoke M1
mandate check --agent invoice-agent --action transfer \
  --resource wallet:treasury-demo --amount 100 --recipient AWS
mandate chain M2
```

```text
✗ DENIED  ANCESTOR_REVOKED

  Agent        invoice-agent
  Request      transfer $100 → AWS  on wallet:treasury-demo
  Revoked      M1 by alice (an ancestor)
  Revocation   0x1f9b1537de358d771a37e9e00b0e4273ae4e4a5caea8c35f12a28a3e2242e0f0
  Delegation   alice → finance-agent → invoice-agent
  Mandate      M2

Chain for M2

  alice
   │  M1  transfer ≤ $1000 → AWS  delegable  expires 2026-10-15 00:47 UTC
   │      ✗ revoked by alice (user_revoked)
   ▼
  finance-agent
   │  M2  transfer ≤ $300 → AWS  expires 2026-10-14 00:47 UTC
   │      ✓ valid
   ▼
  invoice-agent

  Result       ✗ chain broken: M1 is not usable, so M2 grants nothing
```

Other commands: `mandate inspect M2`, `mandate principal list` and `mandate resource list`. Mandates can be referred to as `M1`, by full ID, or by a unique ID prefix. **Exit codes:** `0` success or ALLOW, `1` DENY or rejected delegation, `2` error, so `mandate check` can gate a shell script.

> [!WARNING]
> `.mandate/keys.json` holds **plaintext development private keys** (written owner-only). It's for local demos only and is git-ignored. Never use these keys for anything real.

### HTTP API

`mandate-server` exposes the protocol over JSON. It holds **no private keys**: clients sign mandates and revocations themselves (the CLI's `mandate export` prints them). The server verifies each one before publishing it to Arkiv through the adapter, and decides every request from a fresh snapshot of Arkiv state.

```bash
# terminal 1: the Arkiv adapter (needs apps/arkiv-adapter/.env)
cd apps/arkiv-adapter && npm install && npm start
# terminal 2: the API. Alice's address is the trusted owner of the treasury wallet.
TRUST_ANCHORS="wallet:treasury-demo=0xALICE" cargo run -p mandate-server
```

Or run the whole demo against Arkiv in one go: `scripts/demo-arkiv.sh`.

| Method | Path | Body / result |
|---|---|---|
| `POST` | `/v1/mandates` | signed **root** mandate; must be issued by a trust anchor |
| `POST` | `/v1/mandates/{id}/delegate` | signed child of `{id}`; rejected unless it only narrows a still-valid parent |
| `POST` | `/v1/mandates/{id}/revoke` | signed revocation of `{id}`, by its issuer |
| `POST` | `/v1/authorize` | `{subject, action, resource, amount?, recipient?}` → ALLOW / DENY |
| `GET` | `/v1/mandates/{id}` | the mandate and its status (`ACTIVE` or a deny code) |
| `GET` | `/v1/mandates/{id}/chain` | every link from the root, `valid`, and why not |
| `GET` | `/v1/agents/{address}/mandates` | an agent's mandates with their status |

```bash
curl -s localhost:8080/v1/authorize -H 'content-type: application/json' -d '{
  "subject": "0x81b8…64c1", "action": "transfer",
  "resource": {"kind": "wallet", "id": "treasury-demo"},
  "amount": 700, "recipient": "AWS" }'
```

```json
{
  "decision": "DENY",
  "reason": "AMOUNT_EXCEEDED",
  "message": "Requested 700 exceeds the delegated maximum of 300.",
  "requestedAmount": 700,
  "maximumAmount": 300,
  "mandateId": "0xeafa…134c",
  "delegationPath": ["0x5e16…1ae8", "0x99d6…04b4", "0x81b8…64c1"]
}
```

An ALLOW returns `mandateId`, `rootPrincipal`, `delegationPath`, `chain` and `effectiveConstraints`. Every denial has a stable `reason` code plus its evidence (`revokedMandateId` and `revocationId`, `expiredMandateId`, `allowedRecipients`, …). Some things are deliberate:
- **The server's clock decides expiry.** A client-sent `timestamp` is rejected, so agents can't backdate requests.
- **Unreachable state fails closed.** If Arkiv state can't be read, the answer is `503 STATE_UNAVAILABLE`, never a decision.

### As a library

The same flow is available from Rust. This example is compiled and run as a test ([`readme_flow.rs`](crates/mandate-crypto/tests/readme_flow.rs)):

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


## Protocol reference

### Mandate

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

### Mandate IDs

```text
id = keccak256( canonical_encoding(mandate without id and signature) )
```

The canonical encoding is a hand-written, length-prefixed binary format with the domain separator `mandate/0.1` ([`encoding.rs`](crates/mandate-core/src/encoding.rs)). It never depends on JSON field order. Changing any field that carries authority changes the ID, and the signature is never part of it. The recipient list is treated as a set, so order doesn't matter.

### Signatures

The issuer signs the 32-byte mandate digest with **EIP-191 `personal_sign`** (secp256k1), so any Ethereum wallet or tool can sign a mandate. A test pins a signature produced by Foundry's `cast` to make sure we interoperate.

Verification recomputes the digest from the mandate's content and never trusts the stored ID. It recovers the signer and requires it to equal `issuer`. Verification rejects:

- non-canonical addresses
- an ID that doesn't match the content
- malformed signatures
- malleable high-`s` signatures

## Security model

Mandate v0.1 is designed to reject each of the following. ✅ means it's implemented and covered by tests today.

| Attack | Defence | Status |
|---|---|---|
| Forged mandate | Issuer signature must recover to `issuer` | ✅ |
| Storage tampers with a mandate | ID and signature are recomputed from content | ✅ |
| Privilege escalation through delegation | `can_delegate` attenuation rules | ✅ |
| Self-issued authority (agent grants itself a root mandate) | Root issuer must be a trusted owner of the resource | ✅ |
| Wrong resource or action | Exact match required along the chain | ✅ |
| Expired authority | Signed `expires_at` of every link checked by the engine, not storage | ✅ |
| Amount above the delegated limit | Effective constraints of the chain | ✅ |
| Recipient not allowed | Effective constraints of the chain | ✅ |
| Forged record planted next to a real one | Every candidate verified; closest real reason reported | ✅ |
| Revoked authority | Signed revocation documents | ✅ |
| Ancestor revoked | Full-chain check on every request | ✅ |
| Forged or replayed revocation | Must target the exact mandate, be signed by its issuer, and use the revocation domain separator | ✅ |
| Forged copy of a real mandate published to break a chain | Store returns every copy; the engine uses the one that verifies | ✅ |
| Operator deletes a revocation to revive a mandate | Revocation entities are handed to a burn address in the creating transaction | ✅ (verified on Tiramisu) |

## Threat model

Mandate enforces authorization **when the protected tool honours Mandate's decision**. It does **not** protect against:

- a compromised root private key
- a service or application that ignores Mandate and calls the tool directly
- confidentiality of anything deliberately published to Arkiv (all records are public)
- a malicious operating system or host
- bugs inside the protected external tool
- an LLM being wrong, or every possible prompt-injection attack
- **quantum adversaries:** mandates use secp256k1 ECDSA, like Ethereum itself. A large enough quantum computer could forge these signatures. Post-quantum signatures are out of scope for v0.1.

## Inspiration and related work

Mandate's main inspiration is **Google Zanzibar**, Google's global authorization system. A second Google paper, on **distributed grammars**, is closely related: it deals directly with delegation and authorization in distributed systems.

### Zanzibar: Google's Consistent, Global Authorization System

> Ruoming Pang, Ramón Cáceres, Mike Burrows, Zhifeng Chen, Pratik Dave, Nathan Germer, Alexander Golynski, Kevin Graney, Nina Kang, Lea Kissner, Jeffrey L. Korn, Abhishek Parmar, Christina D. Richards, Mengzhi Wang. *USENIX Annual Technical Conference (ATC)*, 2019.
> [Google Research](https://research.google/pubs/zanzibar-googles-consistent-global-authorization-system/) · [USENIX](https://www.usenix.org/conference/atc19/presentation/pang)

Zanzibar stores and evaluates access-control lists for hundreds of Google services, including Drive, YouTube, Calendar, Cloud and Photos, through one shared data model and one authorization service.

What Mandate takes from Zanzibar:

- **One shared source of authorization truth.** Every service asks the same system "is this allowed?" instead of keeping its own permissions. In Mandate that shared state lives on Arkiv.
- **A generic model.** Zanzibar isn't specific to any one product. Mandate's `action` and `resource` are plain strings for the same reason: payments, GitHub, databases and MCP tools all fit the same model.
- **Answering "who can do what, and why".** Decisions are queries over shared state, and Mandate makes the reasoning explicit in every ALLOW and DENY.

Where Mandate differs:

- **Delegated capabilities, not access-control lists.** Zanzibar stores relations ("user U is an editor of document D"). Mandate stores signed grants that can be narrowed and passed down a chain of agents.
- **No trusted operator.** Zanzibar is run by Google and trusted by the services that call it. Mandate's records are signed by their issuers and re-verified by the engine, so the storage layer can't invent or alter authority.
- **Weaker consistency.** Zanzibar gives strong consistency guarantees, including protection against the "new enemy" problem, where a check uses stale permissions after a revocation. Mandate v0.1 checks against the latest state it reads from Arkiv and makes no such guarantee. Mandate is *inspired by* Zanzibar; it is not a Zanzibar implementation.

### Distributed Authorization with Distributed Grammars

> Martín Abadi, Mike Burrows, Himabindu Pucha, Adam Sadovsky, Asim Shankar, Ankur Taly. In *Programming Languages with Applications to Biology and Security*, Springer, 2015, pp. 10–26. [doi:10.1007/978-3-319-25527-9_3](https://doi.org/10.1007/978-3-319-25527-9_3)
> [Google Research](https://research.google/pubs/distributed-authorization-with-distributed-grammars/)

This paper designs authorization for distributed systems by treating groups as formal languages. Its design supports **delegation**, and it considers group definitions that may not all be available, or may contain cycles.

What Mandate takes from it:

- **Authority that crosses trust boundaries.** Authorization is decided by following delegations between independent parties, not by looking up a single central list. Mandate's check walks the chain Alice → FinanceAgent → InvoiceAgent and verifies every link.
- **Planning for incomplete or hostile state.** Mandate treats a missing parent mandate, an over-long chain or an unverifiable link as an explicit denial (`INVALID_DELEGATION`), never as an error or a silent ALLOW.

Mandate v0.1 does not implement the paper's group grammars or negative clauses. Its delegation model is deliberately simpler: one linear chain per mandate, where each step may only narrow authority.

## Roadmap

- [x] **M1** Core protocol types
- [x] **M2** Canonical encoding and deterministic mandate IDs
- [x] **M3** Delegation validation (`can_delegate`)
- [x] **M4** Ethereum-compatible signatures
- [x] **M5** Authorization engine: requests, decisions, evidence, chain traversal, expiry, trust anchors
- [x] **M6** Revocation and ancestor revocation
- [x] **M7** In-memory store and the full demo flow without Arkiv
- [x] **M8** CLI
- [x] **M9** Arkiv adapter (TypeScript, Tiramisu testnet), [`arkiv/schema.md`](arkiv/schema.md), [`arkiv/friction.md`](arkiv/friction.md)
- [x] **M10** Rust HTTP server, backed by Arkiv
- [ ] **M11** Demo agent: a prompt-injected agent blocked by Mandate
- [ ] **M12** Web UI
- [ ] Public deployment, demo video

Out of scope for v0.1: tokens, NFTs, DAOs, a custom policy language, DIDs, custom chains, ZK proofs, multisig, production money movement.

## Repository layout

```text
mandate/
├── crates/
│   ├── mandate-core/     protocol types, IDs, delegation, revocation, authorization engine (no crypto, no I/O)
│   ├── mandate-crypto/   secp256k1 keys, EIP-191 signing and verification of mandates and revocations
│   ├── mandate-cli/      the `mandate` command
│   └── mandate-server/   HTTP API; decides each request from a fresh Arkiv snapshot
├── apps/
│   └── arkiv-adapter/    TypeScript bridge to Arkiv (stores and queries signed documents)
├── arkiv/
│   ├── schema.md         entities, attributes, queries, what stays off Arkiv
│   └── friction.md       real Arkiv development friction
├── docs/
│   └── decisions.md      every design decision and deviation from the spec
├── scripts/
│   └── demo-arkiv.sh     the full demo against Arkiv Tiramisu
├── AGENTS.md             rules for coding agents working on this repo
└── BUILD_SPEC.md         full v0.1 specification
```

Planned: `apps/web`, `examples/finance-agent`.

## Development

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

All three must pass before any change to authorization logic is merged. Every protocol rule has a test, and tests are never deleted to make CI pass. Design decisions are recorded in [`docs/decisions.md`](docs/decisions.md).

## Hackathon disclosure

Mandate is being built for the **Arkiv: Global Tour Stop** hackathon (Security track, with Open Source as secondary), starting 12 October 2026. All code in this repository was written during the hackathon. Development is AI-assisted (Claude Code) and directed by the author against the specification in [`BUILD_SPEC.md`](BUILD_SPEC.md).

## License

[MIT](LICENSE)
