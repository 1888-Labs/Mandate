# HTTP API

[← Back to the README](../README.md)

`mandate-server` exposes the protocol over JSON. It holds **no private keys**: clients sign mandates and revocations themselves (the CLI's `mandate export` prints them). The server verifies each one before publishing it to Arkiv through the adapter, and decides every request from a fresh snapshot of Arkiv state.

```bash
# terminal 1: the Arkiv adapter (needs apps/arkiv-adapter/.env)
cd apps/arkiv-adapter && npm install && npm start
# terminal 2: the API. Alice's address is the trusted owner of the treasury wallet.
TRUST_ANCHORS="wallet:treasury-demo=0xALICE" cargo run -p mandate-server
```

Or run the whole demo against Arkiv in one go: [`scripts/demo-arkiv.sh`](../scripts/demo-arkiv.sh).

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

An ALLOW returns `mandateId`, `rootPrincipal`, `delegationPath`, `chain` and `effectiveConstraints`. The mandate, chain and agent endpoints also return each record's Arkiv `entityKey` and `$creator`, so a client can check every link on Arkiv for itself. `GET /v1/agents/{address}/mandates?active=true` lists only unexpired mandates, using a range query on the `expires_at` attribute. Every denial has a stable `reason` code plus its evidence (`revokedMandateId` and `revocationId`, `expiredMandateId`, `allowedRecipients`, …). Some things are deliberate:
- **The server's clock decides expiry.** A client-sent `timestamp` is rejected, so agents can't backdate requests.
- **Unreachable state fails closed.** If Arkiv state can't be read, the answer is `503 STATE_UNAVAILABLE`, never a decision.

## Configuration

| Variable | Meaning |
|---|---|
| `ADAPTER_URL` | The Arkiv adapter (default `http://127.0.0.1:8787`) |
| `ADAPTER_TOKEN` | Bearer token for the adapter's write endpoints, if it requires one |
| `TRUST_ANCHORS` | Who may issue root mandates: `kind:id=0xaddr[,0xaddr];…` |
| `MANDATE_BACKEND` | `arkiv` (default) or `memory` (no Arkiv, for local trials) |
| `HOST`, `PORT` | Listen address (default `127.0.0.1:8080`) |

The adapter's own settings (`ARKIV_PRIVATE_KEY`, `ARKIV_ISSUER_KEYS`, `ARKIV_API_KEY`, …) are in [`.env.example`](../.env.example) and [`apps/arkiv-adapter`](../apps/arkiv-adapter).
