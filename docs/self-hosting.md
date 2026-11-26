# Self-hosting with Docker

On Fly.io Sprites the Warden forwards through Sprites connectors and never holds a credential. Anywhere else (a laptop, a VM, a cluster) there are no connectors, so the Warden holds the GitHub token and Ollama key itself. The agent is the same in both cases: it holds only its signing key and can only reach the Warden.

| | On Sprites | In Docker |
|---|---|---|
| GitHub | `GITHUB_CONNECTION_ID` (connector) | `GITHUB_TOKEN` (held by the Warden) |
| Ollama | `OLLAMA_CONNECTION_ID` (connector) | `OLLAMA_URL` (+ `OLLAMA_API_KEY` for Ollama cloud) |
| Checkpoint and restore | `SPRITES_TOKEN` | not available |
| Agent isolation | Sprite network policy | Compose network (add your own egress rules for strict isolation) |

When both a connection ID and a direct credential are set, the connector wins.

## The image

`ghcr.io/1888-labs/mandate` holds three binaries. The default command is the Warden.

```sh
docker run -d -p 8080:8080 -v mandate-data:/data \
  -e WARDEN_ADMIN_TOKEN=$(openssl rand -hex 32) \
  -e GITHUB_TOKEN=github_pat_... \
  ghcr.io/1888-labs/mandate

docker run --rm ghcr.io/1888-labs/mandate warden-call keygen
docker run --rm -e MANDATE_AGENT_KEY=... -e MANDATE_WARDEN_URL=http://host:8080 \
  ghcr.io/1888-labs/mandate mandate-agent "List the open issues"
```

It runs as a non-root user, keeps its state in `/data` (the owner key, mandates, revocations and decision log), and has a health check on `/health`. Keep `/data` on a volume: losing it loses the owner key, and every mandate it signed stops verifying.

## Docker Compose

```sh
cp .env.example .env
# Set WARDEN_ADMIN_TOKEN, plus GITHUB_TOKEN and/or OLLAMA_URL.

docker compose up -d warden                          # dashboard on http://localhost:8080
docker compose run --rm agent warden-call keygen     # copy MANDATE_AGENT_KEY into .env
```

Grant the agent its mandates (the address is printed by `keygen`):

```sh
curl -X POST http://localhost:8080/v1/admin/mandates \
  -H "authorization: Bearer $WARDEN_ADMIN_TOKEN" -H 'content-type: application/json' \
  -d '{"subject":"0xAGENT","action":"read","resource":"github:OWNER/REPO"}'

curl -X POST http://localhost:8080/v1/admin/mandates \
  -H "authorization: Bearer $WARDEN_ADMIN_TOKEN" -H 'content-type: application/json' \
  -d '{"subject":"0xAGENT","action":"model.chat","resource":"ollama:cloud","allowedRecipients":["llama3.2"]}'
```

Then run it:

```sh
MANDATE_REPO=OWNER/REPO MANDATE_MODEL=llama3.2 \
  docker compose run --rm agent mandate-agent "List the open issues"
```

The model resource is always `ollama:cloud`, whether the Ollama behind it is local or hosted. `allowedRecipients` pins the model names the agent may use. The model must support tool calling.

### Local Ollama

Compose maps `host.docker.internal` to the host, so a local Ollama works with:

```sh
OLLAMA_URL=http://host.docker.internal:11434
```

Ollama listens on `127.0.0.1` by default. Set `OLLAMA_HOST=0.0.0.0` for the Ollama server so the container can reach it.

## GitHub token

Use a [fine-grained token](https://github.com/settings/personal-access-tokens) limited to the repositories the agent works on, with the permissions you are willing to delegate (for example Issues and Contents read/write). Mandates narrow what the agent can do within that. They can't grant more than the token allows.

## Build from source

```sh
docker build -t mandate .
MANDATE_IMAGE=mandate docker compose up -d warden
```
