<br/>

<p align="center">
  <a href="https://github.com/1888-Labs/Mandate">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset=".github/logo-dark.svg">
      <img alt="Mandate" src=".github/logo-light.svg" width="auto" height="80">
    </picture>
  </a>
</p>

<p align="center">
  Verifiable delegated authorization for AI agents, on <a href="https://arkiv.network">Arkiv</a>.
</p>

<p align="center">
  <a href="LICENSE">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://img.shields.io/badge/license-MIT-21262d?labelColor=21262d&style=flat">
      <img src="https://img.shields.io/badge/license-MIT-f6f8fa?labelColor=f6f8fa&style=flat" alt="license: MIT">
    </picture>
  </a>
  <a href="https://arkiv.network">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://img.shields.io/badge/built%20on-Arkiv-21262d?labelColor=21262d&style=flat">
      <img src="https://img.shields.io/badge/built%20on-Arkiv-f6f8fa?labelColor=f6f8fa&style=flat" alt="built on: Arkiv">
    </picture>
  </a>
  <a href="arkiv/schema.md">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://img.shields.io/badge/network-Tiramisu%20testnet-21262d?labelColor=21262d&style=flat">
      <img src="https://img.shields.io/badge/network-Tiramisu%20testnet-f6f8fa?labelColor=f6f8fa&style=flat" alt="network: Tiramisu testnet">
    </picture>
  </a>
  <a href="crates/mandate-core">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://img.shields.io/badge/engine-Rust-21262d?labelColor=21262d&style=flat">
      <img src="https://img.shields.io/badge/engine-Rust-f6f8fa?labelColor=f6f8fa&style=flat" alt="engine: Rust">
    </picture>
  </a>
</p>

<br/>

Give an AI agent narrowly scoped, signed authority. Let it hand a *smaller* piece to another agent. Revoke it any time. Every action gets an answer that explains itself: **ALLOW** or **DENY**, with the reason and the chain of authority behind it.

> **The prompt is not the security boundary. Mandate is.**

| | |
|---|---|
| **Live demo** | https://1888-labs.github.io/Mandate/demo/ (API: https://mandate-94oq.onrender.com, on Arkiv Tiramisu) |
| **Demo video** | *coming soon* |
| **Arkiv schema** | [`arkiv/schema.md`](arkiv/schema.md) |
| **Arkiv friction report** | [`arkiv/friction.md`](arkiv/friction.md) |
| **Docs site** | https://1888-labs.github.io/Mandate/ (built with [Vocs](https://vocs.dev), from [`apps/docs`](apps/docs)) · [`docs/`](#documentation) on GitHub |

---

## The problem

AI agents now move money, call APIs and delegate work to *other agents*. A system prompt that says "never spend more than $300" isn't a security boundary: prompt injection, bugs and confused agents talk past it. Nobody can answer: *was this agent actually allowed to do this, who allowed it, and has that been revoked?*

### Why Security

Today the answer lives in the operator's database, and three people are misled by it:

- **The user whose agent overspends.** Alice told her agent "$300, AWS only", but the limit is a line in a prompt or a row someone else can edit. A prompt-injected agent pays whoever the email says.
- **The payee or verifier** deciding whether to accept an agent's request. They can only ask the operator what the agent may do, and must believe the answer.
- **The auditor** reviewing an incident. They see the revocation history the operator shows them, which the operator could have edited or deleted afterwards.

What changes from the first mandate:

- Every grant and revocation is an Arkiv entity created by **the issuer's own wallet**, so its on-chain `$creator` is the issuer. Mandate only accepts a document whose `$creator` equals the issuer named in the signed payload: the adapter checks it ([`arkiv.ts`](apps/arkiv-adapter/src/arkiv.ts) `issuerAuthored`, and `$creator` inside the revocation query), and the Rust server checks it again ([`snapshot.rs`](crates/mandate-server/src/snapshot.rs)). A writable `issuer` field proves nothing; `$creator` can't be forged.
- Revocations are created `readonly` and handed to the burn address `0x…dEaD` in the same transaction, so nobody, not even the issuer or us, can delete one.
- Every check re-verifies signatures, the delegation chain and expiry, and returns a reason code, so a payee or auditor can see *why* (`AMOUNT_EXCEEDED`, `ANCESTOR_REVOKED`, …), not just yes or no.

### Why Open Source

Mandate's state is a public dataset, not a private API. Anyone can read it **without a key or our permission**: query Arkiv for `project_id = "mandate-global-tour-2026"` (`npm run verify` below does exactly that). The data contract is versioned and documented in [`arkiv/schema.md`](arkiv/schema.md): every entity carries `version = "mandate/0.1"` and the full signed document as its payload, so another team can verify mandates with their own code, build a dashboard or an auditor's tool, or run their own adapter. The code is [MIT](LICENSE), and the [CLI](docs/cli.md) and [Rust library](docs/library.md) work on their own, without our server.

## How it works

A **mandate** is a signed grant of authority from one principal (a person or an agent) to another. In this example there are two:

- **M1**: Alice lets FinanceAgent transfer up to $1000, to AWS only, and pass some of that on.
- **M2**: FinanceAgent passes a smaller slice to InvoiceAgent: up to $300, to AWS only.

```mermaid
flowchart LR
    alice(["Alice"]) -->|"M1: up to $1000, to AWS only, may delegate"| finance(["FinanceAgent"])
    finance -->|"M2: up to $300, to AWS only"| invoice(["InvoiceAgent"])
```

| InvoiceAgent asks to send | Mandate says | Why |
|---|---|---|
| $200 → AWS | ✅ ALLOW | Within M2, and M2 only narrows M1 |
| $700 → AWS | ❌ `AMOUNT_EXCEEDED` | Its limit is $300 |
| $100 → RandomVendor | ❌ `RECIPIENT_NOT_ALLOWED` | Only AWS is allowed |
| *(Alice revokes M1)* $100 → AWS | ❌ `ANCESTOR_REVOKED` | M2's authority came from M1 |

Three rules make this work:

1. **Authority only narrows.** A delegated mandate can lower limits, drop recipients or expire sooner, but never more than its parent allows. ([protocol](docs/protocol.md))
2. **Everything is signed.** Each mandate and revocation is signed by its issuer with a standard Ethereum key, and the engine re-verifies the whole chain on every check.
3. **Revoking a parent cuts off everything below it**, instantly, with no need to revoke each child.

## Try it

**Prerequisites:** [Rust](https://rustup.rs) (stable, with `cargo`) for everything. For anything on Arkiv, also Node.js ≥ 22.18 with `npm` (the adapter runs its TypeScript directly), `python3` and `curl` (used by the scripts in `scripts/`).

**1. A prompt-injected agent, stopped (no setup, no network):**

```bash
cargo run -p finance-agent
```

The agent obeys an injected invoice email telling it to send $900, then $250 to an attacker. Its payment tool asks Mandate first, so only the real $200 payment goes through. ([details](examples/finance-agent))

**2. Check our published dataset on Arkiv (read-only, no wallet or key):**

```bash
(cd apps/arkiv-adapter && npm install && npm run verify)
```

See [Verify it on Arkiv](#verify-it-on-arkiv) for what it queries and prints.

**3. The full demo on Arkiv Tiramisu (needs a funded testnet wallet):**

```bash
cp .env.example apps/arkiv-adapter/.env   # set ARKIV_PRIVATE_KEY (fund it at https://hub.arkiv.network/faucet)
                                          # and ARKIV_API_KEY (https://hub.arkiv.network/api-keys)
(cd apps/arkiv-adapter && npm install)
scripts/demo-arkiv.sh
```

This signs mandates with the CLI, publishes them to Arkiv through the Rust API (in the `mandate-global-tour-2026-acceptance` namespace), runs every ALLOW/DENY above, revokes M1 and checks that each result is the expected one.

**4. The web app, locally.** `scripts/run-local.sh` needs demo principals, which a fresh clone doesn't have, so seed first (same `.env` as step 3):

```bash
scripts/seed-arkiv.sh    # once: creates demo principals in .mandate-demo/ and publishes a chain from them
scripts/run-local.sh     # then open http://127.0.0.1:8080
```

`seed-arkiv.sh` generates *your own* demo keys, publishes a fresh Alice → FinanceAgent → InvoiceAgent chain (plus a revoked LegacyBot mandate) from them into the main namespace, and rewrites `arkiv/demo-dataset.json` with your entity keys and transactions. The app then reads that chain from Arkiv. It walks through three steps (who can act for Alice, can it do this, revoke it live) next to a live feed of Arkiv events.

**5. Step by step with the CLI:** see [docs/cli.md](docs/cli.md).

**6. Run your own API with Docker (no Rust or Node needed):**

```bash
docker run --rm -p 8080:8080 -e MANDATE_BACKEND=memory \
  -e TRUST_ANCHORS="wallet:treasury=0xYOUR_ADDRESS" ghcr.io/1888-labs/mandate
```

Then post signed mandates and ask `POST /v1/authorize`. For Arkiv, Docker Compose and production deployments, see [Self-hosting](docs/self-hosting.md) and [Deploying to production](docs/deployment.md). Or call the [public test API](docs/public-api.md) without installing anything.

## Why Arkiv

Mandate needs a public record of who granted what, and what was revoked, that **no single operator controls**.

- **Authorship is on-chain.** Each mandate is an Arkiv entity *created by its issuer's own wallet*, so Arkiv's immutable `$creator` *is* the person who granted the authority. Revocations are looked up by `$creator`, so nobody can publish one "from" Alice.
- **Revocations can't be deleted.** They're handed to a burn address in the same transaction that creates them, so not even their author can quietly undo one.
- **Lifetimes match authorization.** Each entity expires when its mandate does, and typed attributes make "active mandates for this agent" a direct query.

**Without Arkiv, revocation breaks:** a revocation forged by someone other than the issuer, or one an operator quietly deleted, would look exactly like the truth, and a revoked agent would be allowed again. With Postgres, every verifier would have to trust the operator, who could hide a revocation. IPFS can't answer live queries. A subgraph would need a contract to index first. **What stays off Arkiv:** private keys, prompts, tool arguments and personal data. Details: [`arkiv/schema.md`](arkiv/schema.md).

## Verify it on Arkiv

- **Network:** Tiramisu (chain `7738577`), public RPC `https://rpc.tiramisu.db-chain.testnet.arkiv.network`, [explorer](https://tiramisu.explorer.arkiv.network)
- **Namespace:** every entity has `project_id = "mandate-global-tour-2026"`
- **Live dataset (until 2026-11-07):** an active chain Alice → FinanceAgent → InvoiceAgent, and a revoked mandate for LegacyBot. Every entity key and transaction is in [`arkiv/demo-dataset.json`](arkiv/demo-dataset.json).
- **Wallets:** Alice `0xa090f5ffe81d1603015001563b1d465f6131d2a4` and FinanceAgent `0x21418e3a1c39322e2949f5436966498349a144c4` create the entities (`$creator` = issuer). The funder `0xcE64d2DB165BDdc645A3701c58cF4A4490d0f2FB` only pays their gas. ([all wallets](arkiv/schema.md#wallets))

**Read it yourself, with no key:** `npm run verify` in `apps/arkiv-adapter` ([`scripts/verify.ts`](apps/arkiv-adapter/scripts/verify.ts)) runs this query once for `mandate` and once for `revocation`:

```ts
publicClient
  .select({ key: true, owner: true, creator: true, payload: true, expiresAt: true })
  .where(and(
    eq("project_id", str("mandate-global-tour-2026")),
    eq("entity_type", str("mandate")),   // then str("revocation")
  ))
  .limit(200)
```

For each entity it prints the type, mandate ID, the issuer named in the signed payload, `$creator`, `$owner`, the signed expiry, and whether `$creator` equals that issuer. Entities listed in `arkiv/demo-dataset.json` are marked. Expect the three demo mandates created by Alice or FinanceAgent, and one revocation created by FinanceAgent and owned by the burn address `0x…dEaD`; anything else in the namespace (sandbox delegations from the web app, or entities written by strangers) is listed too, and any entity whose `$creator` isn't its issuer is flagged as ignored by Mandate. `ARKIV_API_KEY` is optional; without it the query uses the RPC's anonymous budget.

The engine's own revocation lookup adds the issuer to the query, so only revocations the issuer created can match (the adapter batches several with `or(...)`, see [`arkiv.ts`](apps/arkiv-adapter/src/arkiv.ts)):

```ts
eq("mandate_id", bytes32(mandateId)), eq("$creator", addr(issuer))
```

**After the entities expire:** the creation transactions stay on chain. Each `txHash` in `arkiv/demo-dataset.json` opens at `https://tiramisu.explorer.arkiv.network/tx/<hash>`, or check the sender with [Foundry](https://getfoundry.sh)'s `cast`:

```bash
cast tx 0x469214d527eadd59d442954fbb77b685f059d5d4045e96438b8a9d55b65f5338 from \
  --rpc-url https://rpc.tiramisu.db-chain.testnet.arkiv.network
```

The sender of the root mandate's transaction should be Alice's address above. Details: [evidence that outlives expiry](arkiv/schema.md#evidence-that-outlives-expiry).

The on-chain acceptance test, `npm run test:arkiv` in `apps/arkiv-adapter`, creates the demo chain on Tiramisu and checks each property: issuer authorship, revocation filtering by `$creator`, undeletable revocations and read-only mandates.

## Build with Mandate

| You want to… | Read |
|---|---|
| Try the API without installing anything | [Public test API](docs/public-api.md) |
| Check an agent's action from any language | [HTTP API](docs/api.md) |
| Run your own API (Docker, Render, Fly.io, any server) | [Self-hosting](docs/self-hosting.md) · [Deploying](docs/deployment.md) |
| Grant, delegate and revoke from a terminal | [CLI](docs/cli.md) |
| Embed the engine in Rust | [Library](docs/library.md) |
| Read Mandate's data straight from Arkiv | [Arkiv schema](arkiv/schema.md) |

Protect a tool by asking Mandate before it acts, and only proceeding on ALLOW. That's exactly what the [finance agent's payment tool](examples/finance-agent/src/tool.rs) does.

## Documentation

| | |
|---|---|
| [Protocol](docs/protocol.md) | Mandates, delegation rules, revocation, IDs, signatures, deny codes |
| [Security](docs/security.md) | What Mandate defends against, and what it doesn't |
| [Architecture](docs/architecture.md) | Components, how a decision is made, repository layout |
| [HTTP API](docs/api.md) · [Public test API](docs/public-api.md) · [CLI](docs/cli.md) · [Library](docs/library.md) | Using Mandate |
| [Self-hosting](docs/self-hosting.md) · [Deploying](docs/deployment.md) · [Configuration](docs/configuration.md) · [Troubleshooting](docs/troubleshooting.md) | Running your own API |
| [Development](docs/development.md) | Building, testing, roadmap |
| [Inspiration](docs/inspiration.md) | Google Zanzibar and related work |
| [Arkiv schema](arkiv/schema.md) · [Friction report](arkiv/friction.md) | Arkiv integration |

## Hackathon

Built for **Arkiv: Global Tour Stop** (tracks: **Security** and **Open Source**) between 12 and 18 October 2026. There is no pre-existing work: the baseline is the first commit, [`a0b5245`](https://github.com/1888-Labs/Mandate/commit/a0b5245) (12 October 2026, "Milestone 1: core protocol types"). Early planning documents (`AGENTS.md`, `BUILD_SPEC.md`) were in the history until [`b46234d`](https://github.com/1888-Labs/Mandate/commit/b46234d) stopped tracking them. Development was AI-assisted (Claude Code), directed by the author against a written specification.

## License

[MIT](LICENSE)
