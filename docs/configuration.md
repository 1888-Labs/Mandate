# Configuration

[← Back to the README](../README.md)

Mandate is configured entirely with environment variables. A deployment has two processes. The prebuilt container runs both and wires them together for you:

- **`mandate-server`** (Rust): the public HTTP API and the web UI. It holds no private keys and makes every decision.
- **`arkiv-adapter`** (Node): a private bridge to Arkiv. It reads documents and publishes them from each issuer's wallet.

For Docker, copy [`docker.env.example`](../docker.env.example) to `docker.env`. For a source checkout, the adapter reads `apps/arkiv-adapter/.env` (template: [`.env.example`](../.env.example)), and the server reads its own environment.

## Authorization

| Variable | Default | Meaning |
|---|---|---|
| `TRUST_ANCHORS` | *(empty)* | Who may issue **root** mandates for each resource. Without it, no chain is trusted and every check is `DENY` with `UNTRUSTED_ROOT`. See [format](#trust_anchors). |
| `MANDATE_BACKEND` | `arkiv` | `arkiv`: state lives on Arkiv, through the adapter. `memory`: no Arkiv; state lives in the server and is lost on restart. Use it for trials and tests. |

### `TRUST_ANCHORS`

```text
kind:id=0xowner[,0xowner];kind:id=0xowner
```

```bash
# Alice owns the treasury wallet; two addresses may grant access to the GitHub tool.
TRUST_ANCHORS="wallet:treasury=0xa090f5ffe81d1603015001563b1d465f6131d2a4;mcp:github:create_issue=0x2141…44c4,0x97e8…7772"
```

A resource is `kind:id`. Anything after the first colon is the id, so `mcp:github:create_issue` is kind `mcp`, id `github:create_issue`. A chain is only valid if its root mandate was issued by an owner listed here. This is the one piece of configuration that decides who can grant authority at all, so treat changes to it like changes to an access policy.

## Arkiv

These settings belong to the adapter. The container passes them through.

| Variable | Default | Meaning |
|---|---|---|
| `ARKIV_API_KEY` | *(empty)* | Arkiv access key from [hub.arkiv.network/api-keys](https://hub.arkiv.network/api-keys). **Strongly recommended.** Every decision queries Arkiv, and the anonymous budget is small. |
| `ARKIV_PROJECT_ID` | `mandate-global-tour-2026` | Namespace for every entity and query. Choose your own to keep your mandates apart from everyone else's. |
| `ARKIV_ISSUER_KEYS` | *(empty)* | Comma-separated private keys of the issuers this deployment may publish **as**. Each mandate is created from its issuer's own wallet, so Arkiv's `$creator` proves who granted it. Empty means **read-only**: `authorize` and every read still work, but `POST /v1/mandates`, `/delegate` and `/revoke` fail with `PUBLISH_REJECTED`. |
| `ARKIV_PRIVATE_KEY` | *(empty)* | Optional funder wallet. When an issuer wallet drops below 0.002 GLM, the adapter sends it 0.01 GLM, which covers about 65 writes. It never creates entities itself. |
| `ARKIV_RPC_URL` | Tiramisu | HTTP RPC endpoint. |
| `ARKIV_WS_URL` | Tiramisu | WebSocket RPC endpoint for the live event stream. |

Keys may be given with or without a `0x` prefix. Fund testnet wallets at the [Arkiv faucet](https://hub.arkiv.network/faucet).

> [!WARNING]
> The keys in `ARKIV_ISSUER_KEYS` can grant authority, so guard them like any signing key. If you'd rather not keep them on the server, run it read-only and publish mandates from wherever the keys do live, with the [CLI](cli.md) or the [Rust library](library.md).

## HTTP

| Variable | Default | Meaning |
|---|---|---|
| `HOST` | `127.0.0.1` (`0.0.0.0` in the container) | Listen address. |
| `PORT` | `8080` | Listen port. Hosts like Render and Fly set it for you. |
| `CORS_ORIGINS` | *(empty)* | Comma-separated browser origins allowed to call the API, e.g. `https://app.example.com`. Not needed for server-to-server calls, or for the web UI the server serves at `/`. |
| `WEB_DIR` | `apps/web/dist` (`/app/apps/web/dist` in the container) | The built web UI served at `/`. If it's missing, `/` shows a placeholder and the API still works. |
| `RUST_LOG` | `info` | Log filter, e.g. `info,mandate_server=debug`. |

## Demo sandbox

Only for running the public demo. Leave these empty in your own deployment.

| Variable | Default | Meaning |
|---|---|---|
| `DEMO_KEYS` | *(empty)* | `name=0xkey,…` keys of **synthetic** demo principals, including `finance-agent`. Enables `/v1/demo` and the web UI's "Revoke it live" sandbox. Sandbox writes are capped at 30 per hour. |
| `DEMO_RESOURCE` | `wallet:treasury-demo` | The demo's resource. |

## Running the processes yourself

When you run from source instead of the container, the server finds the adapter with:

| Variable | Default | Meaning |
|---|---|---|
| `ADAPTER_URL` | `http://127.0.0.1:8787` | Where `mandate-server` reaches the adapter. |
| `ADAPTER_TOKEN` | *(empty)* | Shared bearer token. If it's set on the adapter, the adapter only accepts writes that carry it, so set the same value on both processes. The container generates a random one at startup. |

The adapter listens on its own `HOST` and `PORT` (default `127.0.0.1:8787`). Keep it private: it is a backend for `mandate-server`, not a public API.
