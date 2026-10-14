# Arkiv schema

How Mandate stores authorization state on Arkiv. This document describes the code in [`apps/arkiv-adapter`](../apps/arkiv-adapter). If the two ever disagree, the code is right and this file is a bug.

- **Network:** Tiramisu testnet, chain ID `7738577`, RPC `https://rpc.tiramisu.db-chain.testnet.arkiv.network`
- **SDK:** `@arkiv-network/sdk` 0.8.1 (with `viem` 2.57.5)
- **Namespace:** every entity carries `project_id = "mandate-global-tour-2026"`, and every query filters on it. The acceptance test writes to `mandate-global-tour-2026-acceptance` so demo data stays clean.

## Trust model in one paragraph

Every mandate and revocation is protected twice, by two independent checks:

1. **On-chain authorship (`$creator`).** Each entity is created by **its issuer's own wallet**. Alice's grant and Alice's revocation are transactions *from Alice's address*; FinanceAgent's delegation is a transaction from FinanceAgent's address. `$creator` is set by Arkiv and can never change. Mandate only uses entities whose `$creator` is the issuer named inside the document. Revocations are looked up *by* creator inside the Arkiv query (`mandate_id = M AND $creator = issuer`), so entities written by anyone else can't even appear in the results.
2. **Content signatures.** Each document is also signed by its issuer (EIP-191 over a keccak256 canonical encoding, see [`encoding.rs`](../crates/mandate-core/src/encoding.rs)). The Rust engine re-verifies every signature, delegation link and expiry on every check, and checks `$creator == issuer` again itself.

Anyone can write entities into our namespace, and that is safe. A forged copy fails the creator check (and the signature check). A flood of fake revocations can't crowd out the real one, because the revocation query only matches the issuer's own entities. Any other lookup that hits the result cap **fails closed** (`TOO_MANY_RESULTS`) instead of silently truncating.

## Entity: `mandate`

One entity per signed mandate.

| Attribute | Type | Value | Why |
|---|---|---|---|
| `project_id` | `str` | `mandate-global-tour-2026` | Namespace: Arkiv is shared |
| `entity_type` | `str` | `mandate` | Distinguishes mandates from revocations |
| `version` | `str` | `mandate/0.1` | Protocol version of the payload |
| `mandate_id` | `bytes32` | keccak256 of the canonical unsigned mandate | Look up a mandate (and its parent) by ID |
| `parent_id` | `bytes32` | parent's `mandate_id` (**omitted for root grants**) | Find children of a mandate |
| `issuer` | `addr` | issuer's address | Inspection; who granted it |
| `subject` | `addr` | subject's address | Find an agent's mandates |
| `action` | `str` | e.g. `transfer` | Narrow candidates for a check |
| `resource_hash` | `bytes32` | keccak256(u64be(len(kind)) ‖ kind ‖ u64be(len(id)) ‖ id) | Narrow candidates by resource; length-prefixed so `ab`+`c` ≠ `a`+`bc` |
| `resource_kind` | `str` | e.g. `wallet` | Browse by kind of resource |
| `delegable` | `bool` | | Inspection |
| `depth` | `i32` | 0 for a root grant, parent's depth + 1 otherwise; omitted if the parent isn't on Arkiv | Inspection only. It comes from storage, so the engine never trusts it |
| `expires_at` | `u64` | signed `expiresAt` (Unix seconds) | Filter out logically expired mandates at query time |

**Payload** (`application/json`): the complete signed mandate, **exactly the bytes the Rust engine serialized**:

```json
{
  "id": "0x57bde689…49f50",
  "issuer": "0xfcffc882…5c80",
  "subject": "0xafbe5f57…e2d8",
  "action": "transfer",
  "resource": { "kind": "wallet", "id": "treasury-demo" },
  "constraints": { "maxAmount": 300, "allowedRecipients": ["AWS"] },
  "parent": "0xb4072ad7…8739",
  "delegable": false,
  "issuedAt": 1791852453,
  "expiresAt": 1791938853,
  "nonce": 1791852453273973432,
  "signature": "0x…65 bytes…"
}
```

The adapter never parses and re-serializes a payload. `nonce` is a u64 above 2⁵³, so a JavaScript round trip would change it, and that would break the ID and signature (see [friction](friction.md#1-u64-values-in-json-payloads-lose-precision-in-javascript)).

**Creation flags:** `readonly: true`. The content can never be patched. Verified: a patch attempt is rejected with *"was created readonly, so its contents can never change"*.

**Expiration:** `ExpirationTime.atDate(expiresAt)`. The entity disappears from live queries when the mandate expires. **Verified on Tiramisu** (`npm run test:arkiv-expiry`): a 3-minute mandate was still found 1 s before its signed expiry and gone 19 s after (expiry is block-based). The engine still enforces the signed `expiresAt` itself and never relies on Arkiv expiry for correctness.

**Creator and owner:** the issuer's wallet. The issuer could delete their own mandate entity, but that only causes a *denial* (the chain becomes `INVALID_DELEGATION` / missing parent), never a grant. To withdraw authority, the issuer publishes a revocation instead.

## Entity: `revocation`

One entity per signed revocation.

| Attribute | Type | Value | Why |
|---|---|---|---|
| `project_id` | `str` | `mandate-global-tour-2026` | Namespace |
| `entity_type` | `str` | `revocation` | |
| `version` | `str` | `mandate/0.1` | |
| `revocation_id` | `bytes32` | keccak256 of the canonical unsigned revocation | Identity of the revocation |
| `mandate_id` | `bytes32` | the revoked mandate | Find revocations of a mandate |
| `issuer` | `addr` | revoker's address | Inspection; the engine only honours the mandate's own issuer |
| `reason_code` | `str` | e.g. `user_revoked`, or empty | Inspection |

**Payload:** the complete signed revocation, byte for byte:

```json
{
  "id": "0x1f9b1537…e0f0",
  "mandateId": "0xb4072ad7…8739",
  "issuer": "0x44eadfda…d099",
  "revokedAt": 1791852460,
  "reason": "user_revoked",
  "signature": "0x…65 bytes…"
}
```

**Creation flags:** `readonly: true`.

**Expiration:** until the revoked mandate's `expiresAt`, plus a one-hour margin. After that the mandate is expired anyway, and expiry alone denies it. Revoking an ancestor also covers every descendant, because a child can never outlive its parent.

**Creator:** the revoker's wallet, which must be the revoked mandate's issuer. **Owner:** the burn address `0x000000000000000000000000000000000000dEaD`. The issuer's wallet creates the revocation and transfers it **in the same transaction** (`executeBatch` with a key from `predictEntityKey`). `$creator` stays the issuer forever, so authorship is verifiable. Nobody holds the burn address's key, so nobody, *not even the issuer*, can delete the revocation to bring a revoked mandate back to life. **Verified on Tiramisu:** Alice's own delete attempt is rejected with *"entity … is owned by 0x…dEaD"*, while an ordinary entity a wallet still owns *can* be deleted, so the protection comes from the transfer.

## Queries

Every query is `project_id = … AND entity_type = …` plus the filters below. Typed values (`bytes32(…)`, `addr(…)`, `u64(…)`) are used in queries as well as in writes. Results are paged at 200 per page using the query cursor. Past 1000 matches a lookup **fails closed** (`TOO_MANY_RESULTS`). Entities whose payload isn't valid JSON are skipped, and mandate results keep only entities whose `$creator` equals the document's `issuer`.

| Engine needs | Arkiv query (from [`arkiv.ts`](../apps/arkiv-adapter/src/arkiv.ts)) | HTTP |
|---|---|---|
| Every copy of a mandate ID | `eq("mandate_id", bytes32(id))` | `GET /v1/mandates/:id` |
| Candidates for a check | `eq("subject", addr(s))`, `eq("action", str(a))`, `eq("resource_hash", bytes32(h))` | `GET /v1/mandates?subject=&action=&resource=kind:id` |
| An agent's mandates | `eq("subject", addr(s))` | `GET /v1/mandates?subject=` |
| An agent's **active** mandates | the above plus `gt("expires_at", u64(now))`: a range query on the numeric attribute | `GET /v1/mandates?subject=&active=1` |
| Children of a mandate | `eq("parent_id", bytes32(id))` | `GET /v1/mandates?parent=` |
| Every copy of several IDs (one chain level) | `or(eq("mandate_id", bytes32(a)), eq("mandate_id", bytes32(b)), …)`, ≤ 50 IDs | `GET /v1/mandates?id=a&id=b` |
| Revocations of mandates, **by their issuers only** | `or(and(eq("mandate_id", bytes32(a)), eq("$creator", addr(issuerA))), …)`, ≤ 50 | `GET /v1/revocations?of=a:issuerA&of=b:issuerB` |

The Rust server builds each decision from batched lookups: candidates for the agent, one query per chain level for ancestors, and one for all revocations. That's three Arkiv queries for the demo's two-link chain. Verified on Tiramisu: `scripts/demo-arkiv.sh` runs all seven demo steps through these batched queries and checks each result. Queries select only `key`, `owner`, `creator` and `payload`; attributes are read only to compute `depth` when publishing. The RPC charges anonymous callers against a small budget, so the adapter sends `ARKIV_API_KEY` as `X-API-KEY` when set ([friction](friction.md#6-anonymous-query-budget-runs-out-and-the-sdk-then-appears-to-hang)).

Example: candidates for InvoiceAgent's transfer check.

```ts
publicClient
  .select({ key: true, owner: true, payload: true, attributes: true })
  .where(and(
    eq("project_id", str("mandate-global-tour-2026")),
    eq("entity_type", str("mandate")),
    eq("subject", addr("0xafbe5f574857a98764b32fcab3ea959311ece2d8")),
    eq("action", str("transfer")),
    eq("resource_hash", bytes32(resourceHash({ kind: "wallet", id: "treasury-demo" }))),
  ))
  .limit(200)
```

Example: revocations of M1 and M2, each only if published by its own issuer.

```ts
publicClient
  .select({ key: true, owner: true, creator: true, payload: true })
  .where(and(
    eq("project_id", str("mandate-global-tour-2026")),
    eq("entity_type", str("revocation")),
    or(
      and(eq("mandate_id", bytes32(m1.id)), eq("$creator", addr(m1.issuer))),
      and(eq("mandate_id", bytes32(m2.id)), eq("$creator", addr(m2.issuer))),
    ),
  ))
  .limit(200)
```

All lookups return **every** match. The engine needs this: a revocation lookup that returned only one result could be shadowed by a forgery, and so could a mandate-by-ID lookup.

## What stays off Arkiv

Never written to Arkiv: private keys (issuers', agents' or the funder's), seed phrases, API secrets, LLM prompts or reasoning, tool arguments, or personal data. All principals are synthetic. Only signed authorization documents and the public metadata in the tables above. The CLI keeps development keys in a local `keys.json`, and the adapter's keys live in its git-ignored `.env`.

## Why Arkiv, and not…

- **Postgres:** every verifier would have to trust the operator, who could rewrite grants, hide revocations, or show different consumers different answers. On Arkiv, each record's author is fixed on-chain (`$creator`), records are signed, and revocations can't be deleted by anyone, including us.
- **IPFS:** content-addressed blobs, but Mandate needs *live, typed queries* ("active mandates for subject X, action Y, resource Z"; "revocations of M1") and lifetimes that match authorization expiry. IPFS has neither.
- **A subgraph:** a subgraph indexes data that lives somewhere else, so we would first have to invent a contract and events. On Arkiv the authorization records themselves are the first-class, queryable data.

## Verification

`apps/arkiv-adapter` has two on-chain checks, run against Tiramisu on 2026-10-13 with documents freshly signed by the Rust CLI:

```text
$ npm run test:arkiv
✓ 1. create M1 from Alice's own wallet → entity 0x62ad2662…17e8ad
✓ 2. query M1 by ID: byte-for-byte, and $creator is Alice
✓ 3. create M2 → entity 0xc0296081…70918b
✓ 4. query M2 by subject + action + resource (and not by another resource)
✓ 5. query children of M1
✓ 6. create revocation of M1 → entity 0x818828b5…a9958f
✓ 7. query revocation of M1: created by Alice, owned by the burn address
✓ 8. a fake revocation from another wallet is excluded by the $creator filter
✓ 9. re-publishing M1 is a no-op
✓ 10. not even Alice can delete her revocation (burn-address owner)
✓ 11. Alice cannot patch her own mandate entity (read-only)

$ npm run test:arkiv-expiry
✓ queryable before expiry
✓ gone from live queries (19s after its signed expiry)
```

## Wallets

| Address | Role |
|---|---|
| `0xcE64d2DB165BDdc645A3701c58cF4A4490d0f2FB` | **Funder.** Pays gas top-ups to issuer wallets. It created the entities from the earliest test runs; current entities are created by issuers. |
| `0xa090f5ffe81d1603015001563b1d465f6131d2a4` | **Alice**, the demo's trust anchor (owner of `wallet:treasury-demo`). Creator of the root mandate. |
| `0x21418e3a1c39322e2949f5436966498349a144c4` | **FinanceAgent.** Creator of the delegated mandates and of LegacyBot's revocation. |
| `0x97e80edf5d361855f64ff09a3d2cbf37b9f7e772` | **InvoiceAgent**, subject of an active mandate (creates nothing). |
| `0x7f48032178e0c73993546b8c111607001ba4b0dd` | **LegacyBot**, subject of a revoked mandate (creates nothing). |

## Live demo dataset

[`scripts/seed-arkiv.sh`](../scripts/seed-arkiv.sh) publishes the demo chain to the main namespace, checks it, and records every entity key and transaction in [`demo-dataset.json`](demo-dataset.json). It's live until **2026-11-07**, past judging:

| Mandate | Status | Created by |
|---|---|---|
| Alice → FinanceAgent: ≤ $1000 to AWS, delegable | active | Alice |
| FinanceAgent → InvoiceAgent: ≤ $300 to AWS | active | FinanceAgent |
| FinanceAgent → LegacyBot: ≤ $100 to AWS | **revoked** (revocation owned by the burn address) | FinanceAgent |

Every entity expires: a mandate's expiry is part of what its issuer signed, so keeping the demo alive means signing fresh mandates, not extending old entities. Re-running the script does exactly that.

Other demo runs (`scripts/demo-arkiv.sh`, the acceptance tests) use freshly generated issuers and the `-acceptance` namespace.

Example from a live run of `scripts/demo-arkiv.sh` (sender of each transaction, read straight from the chain with `cast tx <hash> from`):

| Transaction | Sent by |
|---|---|
| Alice grants M1 `0xd708d7c79092b217ac7214f1e306ce9c587328ab6df17095db3f2f68fb61154e` | Alice `0x2774E3a0319Cf2Ca7e8b6C8e96d028f811ED8d29` |
| FinanceAgent delegates M2 `0x3c5c6ab3e8a81c49f70c0e3de628772d249920bc47ff385569d927b16e3b6813` | FinanceAgent `0x084CF611ed1531aE1239C63210E52a707B1d821C` |
| Alice revokes M1 `0xbe68b2d3209873aca66b0d2d5a3d6bcbbbd283356506def02d7ef9e32e53b09e` | Alice `0x2774E3a0319Cf2Ca7e8b6C8e96d028f811ED8d29` |

