# Deploying to production

[← Back to the README](../README.md)

Mandate runs anywhere a container runs. This page covers three common setups, a production checklist, and how the public demo is hosted. To try Mandate locally first, see [Self-hosting](self-hosting.md).

Every setup needs the same few settings (full list in [Configuration](configuration.md)):

| Variable | Typical value |
|---|---|
| `TRUST_ANCHORS` | `kind:id=0xowner` for each resource you protect |
| `ARKIV_API_KEY` | your key from [hub.arkiv.network](https://hub.arkiv.network/api-keys) |
| `ARKIV_PROJECT_ID` | a namespace of your own, e.g. `acme-agents-prod` |
| `ARKIV_ISSUER_KEYS` | empty for read-only, or the keys of the issuers that publish through this server |
| `CORS_ORIGINS` | your web app's origin, if a browser calls the API |

## Render

**One click:** [Deploy to Render](https://render.com/deploy?repo=https://github.com/1888-Labs/Mandate). Render reads [`render.yaml`](../render.yaml), builds the Dockerfile, and asks you for the secret values.

**By hand:** New → **Web Service** → your fork of the repository. Language **Docker**, health check path `/health`. Add your variables under **Environment**, and leave `PORT` unset because Render provides it.

> [!NOTE]
> Render's free instances sleep after 15 minutes without traffic, and the first request after that waits about a minute. For anything real, use a paid instance. If you must stay on the free tier, call `/health` every few minutes. [`keep-awake.yml`](../.github/workflows/keep-awake.yml) does this from GitHub Actions.

## Fly.io

```bash
fly launch --image ghcr.io/1888-labs/mandate:latest --internal-port 8080 --no-deploy
fly secrets set TRUST_ANCHORS="wallet:treasury=0x…" ARKIV_API_KEY=… ARKIV_PROJECT_ID=acme-agents-prod
fly deploy
```

Add a health check on `/health` in the generated `fly.toml`, and keep at least one machine running (`min_machines_running = 1`) so checks never wait for a cold start.

## Any server with Docker

On a VM (any cloud, or your own hardware):

```bash
git clone https://github.com/1888-Labs/Mandate && cd Mandate
cp docker.env.example docker.env      # fill it in
docker compose up -d                  # pulls ghcr.io/1888-labs/mandate, restarts on failure
```

Put TLS in front of it. With [Caddy](https://caddyserver.com), this is the whole config:

```text
mandate.example.com {
  reverse_proxy localhost:8080
}
```

Behind nginx, turn off response buffering for `/v1/events` (`proxy_buffering off;`), because it is a Server-Sent Events stream. To upgrade, run `docker compose pull && docker compose up -d`.

## Production checklist

- **Pin the image.** Use `ghcr.io/1888-labs/mandate:sha-<commit>` or a version tag, not `latest`.
- **Use your own namespace.** A unique `ARKIV_PROJECT_ID` means your server only ever reads your entities.
- **Review `TRUST_ANCHORS` like an access policy.** It decides who can start a chain of authority at all.
- **Prefer read-only.** If your tools only check authorizations, leave `ARKIV_ISSUER_KEYS` empty. Publish from wherever your signing keys already live.
- **Guard issuer keys.** If the server does publish, keep the keys in your platform's secret store, never in the image or the repository, and give each issuer wallet only the gas it needs.
- **Set an Arkiv API key.** Every decision reads Arkiv; without a key you share a small anonymous budget, and running out means `503 STATE_UNAVAILABLE`.
- **Don't expose the adapter.** Only `mandate-server`'s port should be reachable. In the container the adapter listens on loopback only.
- **Restrict `CORS_ORIGINS`** to your own web origins, or leave it empty if only servers call the API.
- **Leave `DEMO_KEYS` unset.** Demo mode is for the public demo only.
- **Fail closed in your tools.** Treat anything but `"decision": "ALLOW"` as a no, including timeouts and `5xx`. See [Integrate](https://1888-labs.github.io/Mandate/docs/integrate).
- **Monitor** `GET /health`, and alert on `503` rates from `/v1/authorize`.
- **Keep the logs.** Every decision is logged with its subject, action, resource and mandate ID or deny reason (never keys), which is your audit trail.

## How the public demo is hosted

The public demo is a working example of the setup above.

- **API:** https://mandate-94oq.onrender.com, the same container on Render's free tier, kept awake by [`keep-awake.yml`](../.github/workflows/keep-awake.yml). It runs with `DEMO_KEYS` for the web sandbox and `CORS_ORIGINS=https://1888-labs.github.io`. Its variables are generated from the seeded demo principals by [`scripts/render-env.sh`](../scripts/render-env.sh), after [`scripts/seed-arkiv.sh`](../scripts/seed-arkiv.sh) has published the dataset. All of its keys are synthetic testnet keys.
- **Docs and web UI:** GitHub Pages, built by [`pages.yml`](../.github/workflows/pages.yml). The docs are at https://1888-labs.github.io/Mandate/ and the UI at https://1888-labs.github.io/Mandate/demo/. The UI finds the API through the repository variable `MANDATE_API_URL`:

```bash
gh variable set MANDATE_API_URL --body https://<your-api>
gh workflow run pages.yml
```

- **Image:** [`docker.yml`](../.github/workflows/docker.yml) publishes `ghcr.io/1888-labs/mandate` on every push to `main`.
