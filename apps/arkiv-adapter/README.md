# arkiv-adapter

HTTP bridge between the Mandate Rust engine and [Arkiv](https://arkiv.network) (Tiramisu testnet). It stores and queries **signed** mandates and revocations, and never interprets them: all verification happens in Rust. The schema is in [`arkiv/schema.md`](../../arkiv/schema.md).

Every entity is created **from its issuer's own wallet**, so Arkiv's `$creator` is the issuer. The adapter needs those issuers' keys (`ARKIV_ISSUER_KEYS`, comma-separated) and tops their wallets up with gas from a funder wallet (`ARKIV_PRIVATE_KEY`). Lookups return only entities whose `$creator` is the document's issuer.

```bash
npm install
cp ../../.env.example .env        # then set ARKIV_PRIVATE_KEY (fund it: https://hub.arkiv.network/faucet)
npm start                         # http://127.0.0.1:8787
```

| | |
|---|---|
| `GET /health` | chain ID and writer address |
| `GET /v1/mandates/:id` | every stored copy of a mandate ID |
| `GET /v1/mandates?subject=&action=&resource=kind:id` | an agent's candidate mandates |
| `GET /v1/mandates?parent=` | children of a mandate |
| `GET /v1/mandates?subject=&active=1` | only unexpired mandates (range query on `expires_at`) |
| `GET /v1/revocations?of=<mandate id>:<issuer>` | revocations of a mandate created by its issuer (repeat `of=` to batch) |
| `POST /v1/mandates` | publish a signed mandate (body: the exact JSON from `mandate export`) |
| `POST /v1/revocations?retain_until=<unix>` | publish a signed revocation |

Writes need `Authorization: Bearer $ADAPTER_TOKEN` when `ADAPTER_TOKEN` is set. Documents are returned **byte-for-byte as stored** under `"document"`.

```bash
npm test             # offline unit tests
npm run typecheck
npm run test:arkiv   # on-chain acceptance against Tiramisu (needs a funded key; build the CLI first)
node scripts/probe.ts  # read-only connectivity check
```
