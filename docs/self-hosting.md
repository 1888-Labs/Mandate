# Self-hosting

[← Back to the README](../README.md)

Run your own Mandate API: on your laptop to try it, next to your agents in development, or on a server in production. Everything ships as one container image, `ghcr.io/1888-labs/mandate`, with the HTTP API and the web UI on port `8080`.

## How a deployment fits together

```text
your tools ──▶ mandate-server ──▶ arkiv-adapter ──▶ Arkiv
               public, :8080       private, loopback
               verifies, decides   reads; publishes from
               holds no keys       each issuer's wallet
```

- **`mandate-server`** is the public API. It verifies every signature and decides every request from a fresh read of Arkiv. It never holds a private key.
- **`arkiv-adapter`** is private. It queries Arkiv, and it publishes documents from each issuer's own wallet, so Arkiv's `$creator` proves who granted what.
- **Your tools** call `POST /v1/authorize` before they act, and act only on `ALLOW`.

The container runs both processes and connects them over loopback with a random token.

## Choose a mode

| Mode | Needs | Good for |
|---|---|---|
| **In-memory** (`MANDATE_BACKEND=memory`) | nothing | Trying the API, CI, local development. State is lost on restart. |
| **Arkiv, read-only** | an Arkiv API key | Checking authorizations against mandates someone else publishes. This is how most tools integrate. |
| **Arkiv, read-write** | an API key and your issuers' keys | Running the full system: grant, delegate and revoke through your own API. |

## 1. Try it in five minutes (in-memory)

First create some principals and mandates with the [CLI](cli.md). It makes keys and signs mandates locally:

```bash
cargo install --git https://github.com/1888-Labs/Mandate mandate-cli

mandate principal create alice
mandate principal create finance-agent
mandate principal create invoice-agent
mandate principal list        # note alice's and invoice-agent's addresses

mandate resource own wallet:treasury --owner alice
mandate grant --from alice --to finance-agent --action transfer \
  --resource wallet:treasury --max-amount 1000 --recipient AWS --delegable --expires-in 48h
mandate delegate --mandate M1 --from finance-agent --to invoice-agent \
  --max-amount 300 --expires-in 24h
```

Start the API, trusting Alice as the owner of `wallet:treasury`:

```bash
docker run --rm -p 8080:8080 \
  -e MANDATE_BACKEND=memory \
  -e TRUST_ANCHORS="wallet:treasury=0xALICE_ADDRESS" \
  ghcr.io/1888-labs/mandate
```

http://localhost:8080/health answers `{"ok":true,"backend":"memory"}`, and http://localhost:8080/ is the web UI.

Post the signed mandates. `mandate export` prints each one exactly as it was signed:

```bash
API=http://localhost:8080
M1=$(mandate export M1 | jq -r .id)
mandate export M1 | curl -s $API/v1/mandates -H 'content-type: application/json' -d @-
mandate export M2 | curl -s $API/v1/mandates/$M1/delegate -H 'content-type: application/json' -d @-
```

Now ask the question your tool will ask:

```bash
curl -s $API/v1/authorize -H 'content-type: application/json' -d '{
  "subject": "0xINVOICE_AGENT_ADDRESS",
  "action": "transfer",
  "resource": { "kind": "wallet", "id": "treasury" },
  "amount": 200,
  "recipient": "AWS"
}'
# → {"decision":"ALLOW", …}   with 700: DENY AMOUNT_EXCEEDED
```

Revoke Alice's grant, and InvoiceAgent loses its authority too:

```bash
mandate revoke M1
mandate export M1 --revocations | jq -c '.[0]' \
  | curl -s $API/v1/mandates/$M1/revoke -H 'content-type: application/json' -d @-
# the same authorize call → DENY ANCESTOR_REVOKED
```

## 2. Connect to Arkiv

Get an API key at [hub.arkiv.network/api-keys](https://hub.arkiv.network/api-keys), then use Docker Compose:

```bash
git clone https://github.com/1888-Labs/Mandate && cd Mandate
cp docker.env.example docker.env
# edit docker.env: TRUST_ANCHORS, ARKIV_API_KEY, ARKIV_PROJECT_ID
docker compose up -d
docker compose logs -f
```

```text
arkiv-adapter on http://127.0.0.1:8787  project=mandate-global-tour-2026
read-only: no ARKIV_ISSUER_KEYS, so it cannot publish
mandate-server listening on http://0.0.0.0:8080
```

Read-only is enough to protect tools. To check against the public demo's data, set `ARKIV_PROJECT_ID=mandate-global-tour-2026` and `TRUST_ANCHORS=wallet:treasury-demo=0xa090f5ffe81d1603015001563b1d465f6131d2a4`. InvoiceAgent `0x97e80edf5d361855f64ff09a3d2cbf37b9f7e772` then gets `ALLOW` for $200 to AWS.

For your own data, pick your own `ARKIV_PROJECT_ID` so your queries only ever see your entities.

### Publishing through your server

To grant, delegate and revoke through your own API, give the adapter the keys of the issuers it may publish as:

```bash
ARKIV_ISSUER_KEYS=0xALICE_KEY,0xFINANCE_AGENT_KEY
ARKIV_PRIVATE_KEY=0xFUNDER_KEY   # optional: keeps issuer wallets topped up with gas
```

The server still verifies every document before anything is written, so an invalid or escalating mandate never reaches Arkiv or costs gas. Publishing as an issuer whose key isn't configured fails with `422 PUBLISH_REJECTED` and `"backendError": "ISSUER_KEY_UNAVAILABLE"`.

If you'd rather keep signing keys off the server, leave it read-only and publish from wherever the keys live: the [CLI](cli.md), the [Rust library](library.md), or a small service of your own that holds them.

## 3. Run from source

You need Rust (stable) and Node.js 22.18 or later.

```bash
# the adapter (reads apps/arkiv-adapter/.env, template: .env.example)
cp .env.example apps/arkiv-adapter/.env
cd apps/arkiv-adapter && npm install && npm start

# the API, in another terminal
TRUST_ANCHORS="wallet:treasury=0xALICE" cargo run --release -p mandate-server

# optional: the web UI, served by the API at /
cd apps/web && npm install && npm run build
```

Without Arkiv: `MANDATE_BACKEND=memory TRUST_ANCHORS=… cargo run -p mandate-server`.

[`scripts/run-local.sh`](../scripts/run-local.sh) starts the adapter, the API and the UI against the demo dataset in one command. See [Development](development.md).

## Build the image yourself

```bash
docker build -t mandate .          # or: docker compose build
```

The build compiles the Rust server in release mode. The first build takes 5–10 minutes, and later builds are cached. Tags published to GHCR:

| Tag | What |
|---|---|
| `latest` | The current `main` branch |
| `sha-<commit>` | A specific commit. Pin this in production. |
| `<version>` | Releases, from `v*` tags |

## Next steps

- [Deploy to production](deployment.md): Render, Fly.io or any server, plus a production checklist.
- [Configuration](configuration.md): every environment variable.
- [HTTP API](api.md): every endpoint, request and error.
- [Troubleshooting](troubleshooting.md): common errors and fixes.
