# The Warden

[← Back to the README](../README.md)

The Mandate Warden is an HTTP gateway that sits between an agent and the Fly.io Sprites connectors. It runs on its own Sprite, the only one allowed to use the connectors. The agent runs on a second Sprite whose network is locked so it can only reach the Warden.

For each call, the Warden:

1. Recovers which agent signed the request.
2. Classifies the call into an action, a resource and (sometimes) a recipient.
3. Asks `mandate-core` whether the agent's mandates allow it.
4. On a deny, answers `403` with a reason. On an allow, forwards the call through the connector, which attaches the real credential. The Warden never holds a GitHub or Ollama token.

Every decision is written to a log that the dashboard and `GET /v1/state` read.

## HTTP API

| Route | Auth | What it does |
|---|---|---|
| `GET /` | none | Small status page with the owner address |
| `GET /health` | none | `ok` |
| `GET /v1/state` | none | Owner, labels, mandates, revocations, the last 200 decisions and the known-good checkpoint |
| `POST /v1/mandates` | none | Submit a mandate someone else signed, such as an agent delegating to a sub-agent. Only the signature is checked here; the chain is checked when the mandate is used. Body: `{ "mandate": {...}, "label": "..." }` |
| `POST /v1/admin/mandates` | admin | The owner signs a root mandate (see [Granting mandates](#granting-mandates)) |
| `POST /v1/admin/revoke` | admin | The owner revokes one of its mandates. Body: `{ "mandateId": "0x...", "reason": "..." }` |
| `POST /v1/admin/agent/checkpoint` | admin | Checkpoint the agent Sprite. Body: `{ "comment": "...", "markGood": true }`. With `markGood`, it becomes the restore target |
| `POST /v1/admin/agent/restore` | admin | Restore the agent Sprite. Body: `{ "checkpoint": "v5" }`, or empty to use the known-good one |
| `ANY /v1/gateway/github/{path}` | agent signature | A GitHub REST call, e.g. `/v1/gateway/github/repos/OWNER/REPO/issues` |
| `ANY /v1/gateway/ollama/{path}` | agent signature | An Ollama API call, e.g. `/v1/gateway/ollama/api/chat` |

Admin routes need `Authorization: Bearer $WARDEN_ADMIN_TOKEN`. Without `WARDEN_ADMIN_TOKEN` set, they're disabled (`403 ADMIN_DISABLED`).

## Signing a request

An agent proves who it is with three headers:

| Header | Value |
|---|---|
| `x-mandate-agent` | The agent's address, `0x…` |
| `x-mandate-timestamp` | Unix seconds |
| `x-mandate-signature` | `0x` + 65-byte EIP-191 (`personal_sign`) signature over the digest below |

```text
digest = keccak256(
  "mandate-call/1\n" + METHOD + "\n" + path_and_query + "\n" + timestamp + "\n" + "0x" + hex(keccak256(body))
)
```

`path_and_query` is the full Warden path, starting with `/v1/gateway/`. The Warden treats the recovered signer as the mandate subject, not the claimed address, and rejects the call if they differ. Auth errors return `401` with one of these codes:

- `MISSING_AUTH`
- `BAD_TIMESTAMP`
- `STALE_REQUEST` (more than 60 s of clock skew)
- `BAD_SIGNATURE`
- `SIGNER_MISMATCH`
- `REPLAYED_REQUEST` (a signature can be used only once)

`warden-call` and `mandate-agent` sign for you. Both read `MANDATE_AGENT_KEY` (hex) and `MANDATE_WARDEN_URL`.

```bash
warden-call keygen                                   # new agent key and address
warden-call GET  repos/OWNER/REPO/issues
warden-call POST repos/OWNER/REPO/issues '{"title":"hi"}'
warden-call --ollama POST api/chat '{"model":"gpt-oss:120b","stream":false,"messages":[{"role":"user","content":"hi"}]}'
mandate-agent "Triage the open issues and label anything that is a bug"
```

## How calls are classified

**GitHub.** The resource is `github:OWNER/REPO` for paths under `repos/OWNER/REPO/`, otherwise `github:account`.

| Call | Action | Recipient |
|---|---|---|
| any `GET` / `HEAD` | `read` | |
| `POST issues` | `issues.create` | |
| `POST issues/N/comments` | `issues.comment` | |
| `PATCH issues/N` | `issues.update` | |
| `POST issues/N/labels` | `issues.label` | |
| `POST pulls` | `pulls.create` | base branch |
| `PUT pulls/N/merge` | `pulls.merge` | |
| `PUT contents/...` | `contents.write` | `branch` from the body, default `main` |
| `DELETE contents/...` | `contents.delete` | `branch` from the body, default `main` |
| `POST git/refs` | `refs.create` | the new branch |
| `PATCH` / `DELETE git/refs/heads/NAME` | `refs.update` / `refs.delete` | `NAME` |
| `POST git/blobs`, `git/trees`, `git/commits` | `git.objects` | |
| other `DELETE` | `delete` | |
| write outside a repo | `account.write` | |
| anything else | `other.<method>.<first segment>` | |

**Ollama.** The resource is always `ollama:cloud`, and the recipient is the `model` field. Ollama calls never trigger checkpoints.

| Call | Action |
|---|---|
| `POST api/chat` | `model.chat` |
| `POST api/generate` | `model.generate` |
| `GET api/tags` | `model.list` |
| anything else | `other.<method>.<last segment>` |

The `other.*` actions exist so that no mandate grants them by accident. Anything the classifier doesn't recognise is denied.

## Decisions

An allowed call returns the upstream response with `x-mandate-decision: allow` and `x-mandate-mandate-id`. A denied call returns `403` with `x-mandate-decision: deny`:

```json
{
  "decision": "deny",
  "reason": "RECIPIENT_NOT_ALLOWED",
  "message": "Target main is not one of the allowed: agent/fix.",
  "action": "contents.write",
  "resource": "github:OWNER/REPO",
  "recipient": "main",
  "mandateId": "0x…",
  "delegationPath": ["0x…owner", "0x…agent"],
  "note": "agent restored to its last known-good checkpoint"
}
```

`reason` is a stable code from `mandate-core`: `NO_MANDATE`, `ACTION_NOT_ALLOWED`, `RESOURCE_NOT_ALLOWED`, `RECIPIENT_NOT_ALLOWED`, `AMOUNT_EXCEEDED`, `EXPIRED`, `REVOKED`, `ANCESTOR_REVOKED`, `INVALID_SIGNATURE`, `INVALID_DELEGATION` or `UNTRUSTED_ROOT`. `note` is present only when the Warden did something extra, such as a restore.

Other errors: `502 UPSTREAM_ERROR` (the connector call failed), `503 NO_CONNECTOR` (neither a connection nor a direct credential is configured for that provider), `413 BODY_TOO_LARGE` (over 5 MiB).

## Configuration

| Variable | Default | Purpose |
|---|---|---|
| `PORT` | `8080` | Listen port |
| `WARDEN_DATA` | `warden-data` | Directory for `owner.key`, `store.json` and `decisions.jsonl`. The owner key is created on first run |
| `WARDEN_ADMIN_TOKEN` | unset | Enables the admin routes |
| `GITHUB_CONNECTION_ID` | unset | Sprites GitHub connection to forward through |
| `OLLAMA_CONNECTION_ID` | unset | Sprites Ollama connection to forward through |
| `GITHUB_TOKEN` | unset | Outside Sprites: a GitHub token the Warden calls the API with directly. Ignored when `GITHUB_CONNECTION_ID` is set |
| `GITHUB_API_URL` | `https://api.github.com` | GitHub API base for direct mode (GitHub Enterprise) |
| `OLLAMA_URL` | unset | Outside Sprites: Ollama base URL for direct mode, e.g. `http://host.docker.internal:11434` or `https://ollama.com`. Ignored when `OLLAMA_CONNECTION_ID` is set |
| `OLLAMA_API_KEY` | unset | Bearer key for `OLLAMA_URL` (Ollama cloud) |
| `SPRITES_TOKEN` | unset | Sprites API token; needed for checkpoints and restores |
| `AGENT_SPRITE` | `mandate-agent` | The Sprite to checkpoint and restore |
| `WARDEN_CHECKPOINT_ON_WRITE` | off | Checkpoint the agent before every allowed write (timeout 60 s) |
| `WARDEN_RESTORE_ON_DENY` | off | Restore the agent to the known-good checkpoint after a denied write |
| `SPRITES_GATEWAY` | `https://api.sprites.dev/v1/gateway` | Connector gateway base URL |
| `SPRITES_API` | `https://api.sprites.dev` | Sprites management API base URL |

The agent side reads `MANDATE_AGENT_KEY` and `MANDATE_WARDEN_URL`. `mandate-agent` also reads:

- `MANDATE_REPO` (default `dicethedev/mandate-demo`)
- `MANDATE_MODEL` (default `gpt-oss:120b`)
- `MANDATE_MAX_STEPS` (default `12`)

## Setting up two Sprites

You need the [`sprite` CLI](https://sprites.dev), logged in to your Fly.io organization.

1. **Create two Sprites**: `mandate-warden` labelled `role:warden`, and `mandate-agent` labelled `role:agent`.
2. **Add connectors.** In Sprites, connect GitHub (with access to the repository the agent works on) and Ollama. Note each connection ID.
3. **Lock the connectors to the Warden.** Give each connection an access policy that allows only the Warden: its name prefix `mandate-warden` and label `role:warden`. To check, call the connector gateway from both Sprites. The Warden should get `200` and the agent `403`.
4. **Lock the agent's network.** Set the agent Sprite's network policy to allow only the Warden's URL and deny everything else.
5. **Configure secrets.** Create `.env.sprites` (gitignored) in the repository root with `WARDEN_ADMIN_TOKEN` (any long random string), `GITHUB_CONNECTION_ID`, `OLLAMA_CONNECTION_ID` and `SPRITES_TOKEN`.
6. **Deploy.** Run `scripts/deploy-warden.sh`. It builds in release mode, installs `mandate-warden` on the Warden Sprite as the `warden` service (port 8080, with checkpoint-on-write on), and copies `warden-call` and `mandate-agent` to the agent Sprite's `~/bin`.
7. **Make the agent a key.** On the agent Sprite, run `warden-call keygen`. Keep `MANDATE_AGENT_KEY` on that Sprite only, and set `MANDATE_WARDEN_URL` to the Warden's public URL.
8. **Mark a known-good state.** Run `POST /v1/admin/agent/checkpoint` with `{"markGood": true}`, so restores have a target.

## Granting mandates

The Warden's owner key signs root mandates. `expiresIn` is in seconds and defaults to seven days.

Let the agent think with one model only:

```bash
curl -X POST "$WARDEN/v1/admin/mandates" \
  -H "authorization: Bearer $WARDEN_ADMIN_TOKEN" -H 'content-type: application/json' \
  -d '{
    "subject": "0xAGENT",
    "label": "mandate-agent",
    "action": "model.chat",
    "resource": "ollama:cloud",
    "allowedRecipients": ["gpt-oss:120b"]
  }'
```

Let it read the repository and open issues:

```bash
for action in read issues.create issues.comment; do
  curl -X POST "$WARDEN/v1/admin/mandates" \
    -H "authorization: Bearer $WARDEN_ADMIN_TOKEN" -H 'content-type: application/json' \
    -d "{\"subject\":\"0xAGENT\",\"action\":\"$action\",\"resource\":\"github:OWNER/REPO\"}"
done
```

Let it write files only on its own branches:

```bash
curl -X POST "$WARDEN/v1/admin/mandates" \
  -H "authorization: Bearer $WARDEN_ADMIN_TOKEN" -H 'content-type: application/json' \
  -d '{
    "subject": "0xAGENT",
    "action": "contents.write",
    "resource": "github:OWNER/REPO",
    "allowedRecipients": ["agent/fix"]
  }'
```

Revoke any of them with `POST /v1/admin/revoke`. Revocation takes effect on the next call.
