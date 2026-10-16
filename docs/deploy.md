# Deploying the demo

The public demo has two halves:

- **Web UI** on GitHub Pages: https://1888-labs.github.io/Mandate/. Built and published by [`.github/workflows/pages.yml`](../.github/workflows/pages.yml) on every push to `main` that touches `apps/web`.
- **API** on Railway: one container ([`Dockerfile`](../Dockerfile)) running `mandate-server` on `$PORT` and `arkiv-adapter` on loopback ([`scripts/container-start.sh`](../scripts/container-start.sh)). The server also serves the UI, so the Railway URL works on its own.

The UI finds the API through the repository variable `MANDATE_API_URL`, and the API accepts browser calls only from the origins in `CORS_ORIGINS`.

## API on Railway

1. Seed demo principals once if you haven't: `scripts/seed-arkiv.sh` (needs a funded `apps/arkiv-adapter/.env`).
2. Generate the variables: `scripts/railway-env.sh`. It writes `.mandate-demo/railway.env` (gitignored) and prints only the variable names.
3. Railway → **New Project → Deploy from GitHub repo** → `1888-Labs/Mandate`. Railway picks up [`railway.json`](../railway.json) and builds the `Dockerfile`.
4. Service → **Variables → Raw Editor**: paste the contents of `.mandate-demo/railway.env`.
5. Service → **Settings → Networking → Generate Domain**. Check `https://<domain>/health` and `https://<domain>/v1/demo`.

| Variable | What it is |
| --- | --- |
| `ARKIV_PRIVATE_KEY` | Funder wallet that tops up issuer wallets with gas. Testnet only. |
| `ARKIV_API_KEY` | Arkiv query budget (https://hub.arkiv.network/api-keys). |
| `ARKIV_ISSUER_KEYS` | Alice's and FinanceAgent's keys, so each entity's `$creator` is its issuer. |
| `DEMO_KEYS` | Synthetic demo principals for the web sandbox. |
| `TRUST_ANCHORS` | `wallet:treasury-demo=<Alice's address>`. |
| `CORS_ORIGINS` | `https://1888-labs.github.io` |
| `ARKIV_PROJECT_ID` | Namespace, default `mandate-global-tour-2026`. |

All keys are synthetic testnet demo keys. Never put a real key in these variables.

## Point the UI at the API

```bash
gh variable set MANDATE_API_URL --body https://<domain>
gh workflow run pages.yml
```
