# Arkiv schema

How Mandate stores authorization state on Arkiv. This document describes the code in [`apps/arkiv-adapter`](../apps/arkiv-adapter). If the two ever disagree, the code is right and this file is a bug.

- **Network:** Tiramisu testnet, chain ID `7738577`, RPC `https://rpc.tiramisu.db-chain.testnet.arkiv.network`
- **SDK:** `@arkiv-network/sdk` 0.8.1 (with `viem` 2.57.5)
- **Namespace:** every entity carries `project_id = "mandate-global-tour-2026"`, and every query filters on it. The acceptance test writes to `mandate-global-tour-2026-acceptance` so demo data stays clean.

## Trust model in one paragraph

Arkiv provides **availability and discoverability**, not authority. Every mandate and revocation is a document signed by its issuer (EIP-191 over a keccak256 canonical encoding, see [`encoding.rs`](../crates/mandate-core/src/encoding.rs)). The Rust engine re-verifies every signature, every delegation link and every expiry on every check. Anyone can write entities into our namespace, and that is safe: a forged mandate fails its signature, and a forged revocation is ignored. The engine reads *all* matching entities, so a forgery can never hide a genuine record.

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

**Owner:** the adapter's wallet. Deleting a mandate entity can only cause a *denial* (the chain becomes `INVALID_DELEGATION` / missing parent), never a grant.

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

**Owner:** the burn address `0x000000000000000000000000000000000000dEaD`. The revocation is created and transferred **in the same transaction** (`executeBatch` with a key from `predictEntityKey`). Nobody holds that key, so nobody, including the adapter operator, can delete a revocation to bring a revoked mandate back to life. **Verified on Tiramisu:** the acceptance test's delete attempt from the adapter wallet is rejected with *"entity … is owned by 0x…dEaD, not 0xcE64…"*. A control entity owned by the adapter *can* be deleted, so the protection comes from the ownership transfer.

## Queries

Every query is `project_id = … AND entity_type = …` plus the filters below. Typed values (`bytes32(…)`, `addr(…)`) are used in queries as well as in writes. Results are paged at 200 per page and capped at 1000, against spam in the namespace. Entities whose payload isn't valid JSON are skipped.

| Engine needs | Arkiv query (from [`arkiv.ts`](../apps/arkiv-adapter/src/arkiv.ts)) | HTTP |
|---|---|---|
| Every copy of a mandate ID | `eq("mandate_id", bytes32(id))` | `GET /v1/mandates/:id` |
| Candidates for a check | `eq("subject", addr(s))`, `eq("action", str(a))`, `eq("resource_hash", bytes32(h))` | `GET /v1/mandates?subject=&action=&resource=kind:id` |
| An agent's mandates | `eq("subject", addr(s))` | `GET /v1/mandates?subject=` |
| Children of a mandate | `eq("parent_id", bytes32(id))` | `GET /v1/mandates?parent=` |
| Revocations of a mandate | `eq("mandate_id", bytes32(id))` on `entity_type = revocation` | `GET /v1/revocations?mandate=` |
| Every copy of several IDs (one chain level) | `or(eq("mandate_id", bytes32(a)), eq("mandate_id", bytes32(b)), …)`, ≤ 50 IDs | `GET /v1/mandates?id=a&id=b` |
| Revocations of several mandates | the same OR on `entity_type = revocation`, ≤ 50 IDs | `GET /v1/revocations?mandate=a&mandate=b` |

The Rust server builds each decision from batched lookups: candidates for the agent, one query per chain level for ancestors, and one for all revocations. That's three Arkiv queries for the demo's two-link chain. Verified on Tiramisu: `scripts/demo-arkiv.sh` runs all seven demo steps through these batched queries and checks each result. Queries select only `key`, `owner` and `payload`; attributes are read only to compute `depth` when publishing. The RPC charges anonymous callers against a small budget, so the adapter sends `ARKIV_API_KEY` as `X-API-KEY` when set ([friction](friction.md#6-anonymous-query-budget-runs-out-and-the-sdk-then-appears-to-hang)).

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

All lookups return **every** match. The engine needs this: a revocation lookup that returned only one result could be shadowed by a forgery, and so could a mandate-by-ID lookup ([decisions](../docs/decisions.md)).

## What stays off Arkiv

Never written to Arkiv: private keys (agents' or the adapter's), seed phrases, API secrets, LLM prompts or reasoning, tool arguments, or personal data. Only signed authorization documents and the public metadata in the tables above. The CLI keeps development keys in a local `keys.json`, and the adapter's key lives in its git-ignored `.env`.

## Why Arkiv, and not…

- **Postgres:** every verifier would have to trust the operator, who could rewrite grants, hide revocations, or show different consumers different answers. On Arkiv, records are public and signed, and revocations can't even be deleted by us.
- **IPFS:** content-addressed blobs, but Mandate needs *live, typed queries* ("active mandates for subject X, action Y, resource Z"; "revocations of M1") and lifetimes that match authorization expiry. IPFS has neither.
- **A subgraph:** a subgraph indexes data that lives somewhere else, so we would first have to invent a contract and events. On Arkiv the authorization records themselves are the first-class, queryable data.

## Verification

`apps/arkiv-adapter` has two on-chain checks, run against Tiramisu on 2026-10-13 with documents freshly signed by the Rust CLI:

```text
$ npm run test:arkiv
✓ 1. create M1 → entity 0xe5272b89…f75704
✓ 2. query M1 by ID (returned byte-for-byte)
✓ 3. create M2 → entity 0x83b9a4c1…0d6111
✓ 4. query M2 by subject + action + resource (and not by another resource)
✓ 5. query children of M1
✓ 6. create revocation of M1 → entity 0x5ea1e242…c48875
✓ 7. query revocation of M1 (owned by the burn address)
✓ 8. re-publishing M1 is a no-op
✓ 9. adapter wallet cannot delete the revocation
✓ 10. mandate entity is read-only (patch rejected)

$ npm run test:arkiv-expiry
✓ queryable before expiry
✓ gone from live queries (19s after its signed expiry)
```

Writer (creator) address for all Mandate entities: `0xcE64d2DB165BDdc645A3701c58cF4A4490d0f2FB`.

