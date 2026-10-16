# HTTP API

[← Back to the README](../README.md)

`mandate-server` exposes the whole protocol over JSON: issue, delegate and revoke mandates, and ask whether an agent may act. You can run your own ([Self-hosting](self-hosting.md)) or try the [public test API](public-api.md).

```text
Self-hosted:  http://localhost:8080
Public test:  https://mandate-94oq.onrender.com
```

## Conventions

- **JSON in, JSON out.** Send `content-type: application/json`. Field names are `camelCase`.
- **No API keys, no sessions.** Writes are authorized by **signatures**: you sign each mandate or revocation with the issuer's key before sending it, and the server verifies it. The server never holds your keys. Reads and `authorize` are open; put the server behind your network or gateway if they shouldn't be.
- **Addresses** are `0x` followed by 40 hex characters; the server returns them lowercase. **Mandate and revocation IDs** are `0x` followed by 64 lowercase hex characters (keccak256 of the canonical document).
- **Resources** are objects `{ "kind": "wallet", "id": "treasury" }`; in configuration and the CLI they're written `wallet:treasury`.
- **Amounts** are non-negative integers in whatever unit your tool uses (cents, wei, tokens). Mandate only compares them.
- **The server's clock decides.** Expiry is checked against server time, and requests can't carry their own timestamp.
- **Unknown fields are rejected** on `POST /v1/authorize`, so typos fail loudly instead of being ignored.

### Errors

A request that can't be processed returns a non-2xx status with a stable `error` code and a readable `message`, sometimes plus details:

```json
{ "error": "UNTRUSTED_ROOT", "message": "the issuer is not a trusted owner of this resource" }
```

A **decision** is not an error. `POST /v1/authorize` answers `200` with `"decision": "DENY"` and a `reason` from [Deny codes](https://1888-labs.github.io/Mandate/docs/deny-codes). All error codes are listed [below](#error-codes).

## Endpoints

| Method | Path | Purpose |
|---|---|---|
| `POST` | [`/v1/authorize`](#post-v1authorize) | May this agent do this? ALLOW or DENY, with evidence |
| `POST` | [`/v1/mandates`](#post-v1mandates) | Publish a signed root mandate |
| `POST` | [`/v1/mandates/{id}/delegate`](#post-v1mandatesiddelegate) | Publish a signed child of `{id}` |
| `POST` | [`/v1/mandates/{id}/revoke`](#post-v1mandatesidrevoke) | Publish a signed revocation of `{id}` |
| `GET` | [`/v1/mandates/{id}`](#get-v1mandatesid) | A mandate and its current status |
| `GET` | [`/v1/mandates/{id}/chain`](#get-v1mandatesidchain) | Every link from the root to `{id}` |
| `GET` | [`/v1/agents/{address}/mandates`](#get-v1agentsaddressmandates) | An agent's mandates and their status |
| `GET` | [`/v1/events`](#get-v1events) | Live Arkiv events (Server-Sent Events) |
| `GET` | [`/health`](#get-health) | Liveness and backend |

### `POST /v1/authorize`

The one call your tools make before they act.

| Field | Type | |
|---|---|---|
| `subject` | address | The agent asking. Your tool decides who that is (its session, API key or signed request), never the agent's prompt. |
| `action` | string | e.g. `transfer`, `create_issue`, `deploy` |
| `resource` | `{kind, id}` | What it acts on |
| `amount` | integer, optional | Required if any mandate in the chain sets `maxAmount` |
| `recipient` | string, optional | Required if any mandate in the chain restricts recipients |

```bash
curl -s $API/v1/authorize -H 'content-type: application/json' -d '{
  "subject": "0x97e80edf5d361855f64ff09a3d2cbf37b9f7e772",
  "action": "transfer",
  "resource": { "kind": "wallet", "id": "treasury-demo" },
  "amount": 200,
  "recipient": "AWS"
}'
```

`200`, allowed. Keep this as your audit evidence:

```json
{
  "decision": "ALLOW",
  "mandateId": "0x614f35e041ade2442e1f925c966ec3c139242f1a833e92c1e67423c471df6527",
  "rootPrincipal": "0xa090f5ffe81d1603015001563b1d465f6131d2a4",
  "delegationPath": [
    "0xa090f5ffe81d1603015001563b1d465f6131d2a4",
    "0x21418e3a1c39322e2949f5436966498349a144c4",
    "0x97e80edf5d361855f64ff09a3d2cbf37b9f7e772"
  ],
  "chain": [
    "0x7dea259529bd149699911f22305b76fe5a3827fa1537d60b52e4d8099cd2024d",
    "0x614f35e041ade2442e1f925c966ec3c139242f1a833e92c1e67423c471df6527"
  ],
  "effectiveConstraints": { "maxAmount": 300, "allowedRecipients": ["AWS"] }
}
```

`200`, denied, with `amount: 700`:

```json
{
  "decision": "DENY",
  "reason": "AMOUNT_EXCEEDED",
  "message": "Requested 700 exceeds the delegated maximum of 300.",
  "requestedAmount": 700,
  "maximumAmount": 300,
  "mandateId": "0x614f35e041ade2442e1f925c966ec3c139242f1a833e92c1e67423c471df6527",
  "delegationPath": ["0xa090…d2a4", "0x2141…44c4", "0x97e8…7772"]
}
```

Each `reason` comes with its own evidence fields (`revokedMandateId` and `revocationId`, `expiredMandateId` and `expiresAt`, `allowedRecipients`, …). See [Deny codes](https://1888-labs.github.io/Mandate/docs/deny-codes).

Errors: `400 INVALID_REQUEST`, `503 STATE_UNAVAILABLE`. **Treat every non-ALLOW outcome as a no**, including errors and timeouts.

### `POST /v1/mandates`

Publish a signed **root** mandate: one with no parent, issued by a [trust anchor](configuration.md#trust_anchors) of its resource. The body is the [mandate document](#mandate-document).

```bash
mandate export M1 | curl -s $API/v1/mandates -H 'content-type: application/json' -d @-
```

```json
{
  "mandateId": "0x1f97…daf8",
  "published": { "created": true, "entityKey": "0x…", "txHash": "0x…" }
}
```

`201` when it was stored, or `200` with `"created": false` if the identical document already was. Posting the same mandate again is safe. On the in-memory backend `entityKey` and `txHash` are `null`.

Errors: `400 INVALID_SIGNATURE`, `400 NOT_A_ROOT_MANDATE`, `403 UNTRUSTED_ROOT`, `400 EXPIRED`, `422 PUBLISH_REJECTED`, `503 STATE_UNAVAILABLE`.

### `POST /v1/mandates/{id}/delegate`

Publish a signed child of mandate `{id}`. It is checked before anything is written. The parent must still grant authority, and the child must only **narrow** it: same action and resource, no higher limit, no new recipients, no later expiry, and issued by the parent's subject.

```bash
mandate export M2 | curl -s $API/v1/mandates/$M1/delegate -H 'content-type: application/json' -d @-
```

Response as for `POST /v1/mandates`.

Errors: `400 PARENT_MISMATCH` (the body's `parent` isn't `{id}`), `400 INVALID_SIGNATURE`, `404 MANDATE_NOT_FOUND`, `409 PARENT_UNUSABLE` (the parent is revoked, expired or otherwise invalid; `parentDenial` says why), `422 DELEGATION_REJECTED` (`reason`: `PARENT_NOT_DELEGABLE`, `INVALID_ISSUER`, `INVALID_PARENT`, `ACTION_ESCALATION`, `RESOURCE_ESCALATION`, `AMOUNT_ESCALATION`, `RECIPIENT_ESCALATION` or `EXPIRATION_ESCALATION`), `400 EXPIRED`, `422 PUBLISH_REJECTED`, `503 STATE_UNAVAILABLE`.

### `POST /v1/mandates/{id}/revoke`

Publish a signed [revocation](#revocation-document) of `{id}`. Only the mandate's issuer may revoke it. Everything delegated beneath it stops on the next check.

```bash
mandate revoke M1
mandate export M1 --revocations | jq -c '.[0]' \
  | curl -s $API/v1/mandates/$M1/revoke -H 'content-type: application/json' -d @-
```

```json
{ "revocationId": "0xb424…8924", "mandateId": "0x1f97…daf8", "published": { "created": true, … } }
```

On Arkiv, revocations are handed to the burn address once published, so nobody can delete them, not even their issuer.

Errors: `400 MANDATE_MISMATCH`, `400 INVALID_SIGNATURE`, `404 MANDATE_NOT_FOUND`, `403 NOT_MANDATE_ISSUER`, `422 PUBLISH_REJECTED`, `503 STATE_UNAVAILABLE`.

### `GET /v1/mandates/{id}`

The mandate, its status right now (`ACTIVE` or a deny code), and where it lives on Arkiv.

```json
{
  "mandate": { "id": "0xa133…01ac", "issuer": "0x2141…44c4", "subject": "0x7f48…b0dd", … },
  "status": "REVOKED",
  "denial": {
    "reason": "REVOKED",
    "message": "The agent's mandate has been revoked.",
    "revokedMandateId": "0xa133…01ac",
    "revocationId": "0xdb8a…7ff26",
    "delegationPath": ["0xa090…d2a4", "0x2141…44c4", "0x7f48…b0dd"]
  },
  "arkiv": { "entityKey": "0x919b…5962", "creator": "0x2141…44c4", "owner": "0x2141…44c4" }
}
```

`arkiv.creator` is Arkiv's immutable `$creator`. It equals the issuer, which anyone can check on the explorer. Errors: `400 INVALID_PARAMETER`, `404 MANDATE_NOT_FOUND`, `503 STATE_UNAVAILABLE`.

### `GET /v1/mandates/{id}/chain`

Every link from the root to `{id}`, whether or not the chain is valid, so you can show *why*.

| Field | |
|---|---|
| `valid` | `true` if the chain grants authority now |
| `links` | the mandates, root first |
| `delegationPath` | the principals, root first |
| `effectiveConstraints` | the tightest limits along the chain (when valid) |
| `denial` | why not (when invalid) |
| `arkiv` | `{ mandateId: { entityKey, creator, owner } }` for each link |
| `revocations` | any valid revocations of links, with their Arkiv refs |

### `GET /v1/agents/{address}/mandates`

The mandates held by an agent, each with its `status`, `denial` and `arkiv` ref as in `GET /v1/mandates/{id}`. Add `?active=true` for only those that haven't expired. That filter is a range query on Arkiv, not a filter applied afterwards.

```json
{ "agent": "0x97e8…7772", "mandates": [ { "mandate": { … }, "status": "ACTIVE", "arkiv": { … } } ] }
```

Only copies published by their own issuer are returned. Anything else on Arkiv claiming to be a mandate is ignored.

### `GET /v1/events`

A Server-Sent Events stream of mandates and revocations landing on Arkiv for the issuers this server publishes as. Available with the Arkiv backend only.

```text
event: ready
data: {"watching":["0xa090…d2a4","0x2141…44c4"]}

event: arkiv
data: {"kind":"revocation","action":"created","entityKey":"0x…","creator":"0x…","mandateId":"0x…","txHash":"0x…","blockNumber":"…"}
```

### `GET /health`

```json
{ "ok": true, "backend": "arkiv" }
```

`backend` is `arkiv` or `memory`. Use it for load balancer and uptime checks.

## Documents

Mandates and revocations are signed JSON documents. Create and sign them with the [CLI](cli.md) (`mandate grant`, `delegate`, `revoke`, then `mandate export`) or the [Rust library](library.md). The signature covers a canonical encoding, so a document can't be changed after signing; see [Protocol](protocol.md).

### Mandate document

```json
{
  "id": "0x614f35e041ade2442e1f925c966ec3c139242f1a833e92c1e67423c471df6527",
  "issuer": "0x21418e3a1c39322e2949f5436966498349a144c4",
  "subject": "0x97e80edf5d361855f64ff09a3d2cbf37b9f7e772",
  "action": "transfer",
  "resource": { "kind": "wallet", "id": "treasury-demo" },
  "constraints": { "maxAmount": 300, "allowedRecipients": ["AWS"] },
  "parent": "0x7dea259529bd149699911f22305b76fe5a3827fa1537d60b52e4d8099cd2024d",
  "delegable": false,
  "issuedAt": 1791947670,
  "expiresAt": 1794021270,
  "nonce": 1791947670468365344,
  "signature": "0x3869…591b"
}
```

| Field | |
|---|---|
| `id` | keccak256 of the canonical document without its signature |
| `issuer`, `subject` | who grants, who receives |
| `action`, `resource` | what may be done, to what |
| `constraints` | `maxAmount` and `allowedRecipients`; `null` means unrestricted |
| `parent` | the mandate this narrows, or `null` for a root |
| `delegable` | whether the subject may pass a slice on |
| `issuedAt`, `expiresAt` | Unix seconds |
| `nonce` | makes otherwise identical grants distinct |
| `signature` | the issuer's secp256k1 signature, hex |

### Revocation document

```json
{
  "id": "0xdb8a440ee952ce55cd19646fad60c1dfe034f5fb70200c694e931ac0bdc7ff26",
  "mandateId": "0xa1331ff6058879869a7d8bc48173a5aa2ce80fec6deeb7fa10939a3eeb7701ac",
  "issuer": "0x21418e3a1c39322e2949f5436966498349a144c4",
  "revokedAt": 1791947680,
  "reason": "retired",
  "signature": "0x…"
}
```

A revocation takes effect as soon as it's published. `revokedAt` is for information only.

## Error codes

| Status | `error` | Meaning |
|---|---|---|
| 400 | `INVALID_REQUEST` | The body isn't valid JSON for this endpoint, or has unknown fields |
| 400 | `INVALID_PARAMETER` | A path parameter isn't a valid ID or address |
| 400 | `INVALID_SIGNATURE` | The document's signature doesn't match its issuer and contents |
| 400 | `NOT_A_ROOT_MANDATE` | A mandate with a parent was sent to `POST /v1/mandates`; use `/delegate` |
| 400 | `PARENT_MISMATCH` | The child's `parent` isn't the mandate in the URL |
| 400 | `MANDATE_MISMATCH` | The revocation's `mandateId` isn't the mandate in the URL |
| 400 | `EXPIRED` | The document has already expired |
| 403 | `UNTRUSTED_ROOT` | The root's issuer isn't a trust anchor for its resource |
| 403 | `NOT_MANDATE_ISSUER` | Only a mandate's issuer may revoke it |
| 404 | `MANDATE_NOT_FOUND` | No valid, issuer-published mandate with that ID |
| 404 | `NOT_FOUND` | No such endpoint |
| 409 | `PARENT_UNUSABLE` | The parent no longer grants authority (`parentDenial` says why) |
| 422 | `DELEGATION_REJECTED` | The child would widen its parent's authority (`reason` says how) |
| 422 | `PUBLISH_REJECTED` | Arkiv refused the write (`backendError`, e.g. `ISSUER_KEY_UNAVAILABLE` on a read-only server) |
| 503 | `STATE_UNAVAILABLE` | Arkiv state couldn't be read or written. Never treat this as a decision. |

Demo-mode endpoints (`/v1/demo/…`) can also return `DEMO_DISABLED`, `DEMO_RATE_LIMITED`, `NO_ROOT_MANDATE` and `NOT_A_SANDBOX_MANDATE`.

## Demo mode

With [`DEMO_KEYS`](configuration.md#demo-sandbox) set, the server also exposes the web demo's sandbox:

| Method | Path | |
|---|---|---|
| `GET` | `/v1/demo` | The demo's principals, resource and sandbox limits |
| `POST` | `/v1/demo/sandbox` | FinanceAgent delegates ≤ $50 → AWS for 2 hours to a fresh agent |
| `POST` | `/v1/demo/sandbox/{id}/revoke` | FinanceAgent revokes that sandbox mandate (sandbox mandates only) |

Sandbox writes are limited to 30 per hour in total, because each one spends testnet gas.
