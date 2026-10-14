# Architecture

[← Back to the README](../README.md)

```mermaid
flowchart TD
    issuers["<b>Issuers</b><br/>Alice, FinanceAgent<br/>sign mandates and revocations"]
    agent["<b>AI agent</b><br/>e.g. InvoiceAgent"]
    tool["<b>Protected tool</b><br/>e.g. payments"]
    server["<b>mandate-server</b> (Rust)<br/>all authorization logic"]
    engine["<b>Authorization engine</b><br/>signatures · delegation chain · trust anchors<br/>expiry · revocations · limits"]
    adapter["<b>arkiv-adapter</b> (TypeScript)<br/>publishes from issuer wallets<br/>filters by $creator"]
    arkiv[("<b>Arkiv</b> · Tiramisu testnet<br/>mandate and revocation entities")]

    issuers -->|"signed documents"| server
    agent -->|"transfer $200 → AWS"| tool
    tool -->|"POST /v1/authorize"| server
    server -->|"snapshot of the relevant records"| engine
    engine -->|"ALLOW / DENY + reason"| tool
    server <-->|"read · publish"| adapter
    adapter <-->|"SDK queries · transactions"| arkiv
```

Rust owns what authorization *means*. TypeScript only moves records to and from Arkiv, because the official Arkiv SDK is TypeScript.



## How a decision is made

```mermaid
sequenceDiagram
    participant T as Payment tool
    participant S as mandate-server
    participant A as arkiv-adapter
    participant K as Arkiv
    T->>S: POST /v1/authorize<br/>InvoiceAgent: transfer $200 → AWS
    S->>A: InvoiceAgent's mandates for this action and resource
    A->>K: query subject + action + resource
    K-->>A: M2, created by FinanceAgent
    A-->>S: M2
    S->>A: every copy of M2's parent, M1
    A->>K: query mandate_id = M1
    K-->>A: M1, created by Alice
    A-->>S: M1
    S->>A: revocations of M1 and M2, by their issuers only
    A->>K: query (mandate_id, $creator) pairs
    K-->>A: none
    A-->>S: none
    S->>S: verify signatures, chain, trust anchor,<br/>expiry, revocations and limits
    S-->>T: ALLOW, via Alice → FinanceAgent → InvoiceAgent
```

For each request, `mandate-server` fetches from Arkiv the agent's candidate mandates, every copy of each ancestor (one batched query per chain level), and the revocations of all of them published by their issuers (one batched query). It loads them into an in-memory snapshot and runs the same synchronous engine the CLI and tests use. For the demo chain that's three Arkiv queries and about 0.65 s per decision.

- **Fail closed.** If Arkiv can't be read, the answer is `503 STATE_UNAVAILABLE`, never a decision. Anything missing from a snapshot is a denial.
- **No keys on the server.** Clients sign documents themselves; the server verifies them before publishing.
- **Two independent checks on every record:** its on-chain `$creator` must be its issuer, and its signature must verify. See [`arkiv/schema.md`](../arkiv/schema.md).

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
├── examples/
│   └── finance-agent/    a prompt-injected agent whose payments Mandate stops
└── scripts/
    └── demo-arkiv.sh     the full demo against Arkiv Tiramisu
```

