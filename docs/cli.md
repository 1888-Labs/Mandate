# CLI

[← Back to the README](../README.md)

Mandate ships four command-line tools. Three of them make up the running system: a Warden, and an agent that can only act through it. The fourth, `mandate`, lets you design and test mandates on your own machine before you grant them.

| Tool | Runs on | What it does |
|---|---|---|
| [`mandate-warden`](#mandate-warden) | The Warden Sprite, or Docker | The gateway: verifies, classifies, authorizes and logs every call, and serves the dashboard |
| [`warden-call`](#warden-call) | The agent Sprite | Makes one signed call through the Warden. Also creates agent keys |
| [`mandate-agent`](#mandate-agent) | The agent Sprite | A coding agent that thinks with Ollama and acts on GitHub, both only through the Warden |
| [`mandate`](#mandate) | Your machine | Signs, delegates, revokes and checks mandates locally |

## Install

The first three come from the `mandate-warden` crate, and `mandate` from `mandate-cli`:

```bash
cargo install --path crates/mandate-warden   # mandate-warden, warden-call, mandate-agent
cargo install --path crates/mandate-cli      # mandate
```

The Docker image `ghcr.io/1888-labs/mandate` contains `mandate-warden`, `warden-call` and `mandate-agent`. On Sprites, `scripts/deploy-warden.sh` builds them and installs each one on the right Sprite (see [Setting up two Sprites](warden.md#setting-up-two-sprites)).

## mandate-warden

The Warden server. It takes no arguments and is configured entirely with environment variables (see [Configuration](warden.md#configuration)).

```bash
WARDEN_ADMIN_TOKEN=change-me GITHUB_TOKEN=ghp_… mandate-warden   # listens on :8080
```

On first run it creates the owner key in `WARDEN_DATA` (default `./warden-data`). That key is the only trusted root: every mandate the Warden honours must lead back to it. The dashboard is at `/`.

The usual ways to run it:

```bash
docker compose up -d warden      # Docker, with Warden-held credentials (see Self-hosting)
scripts/deploy-warden.sh         # Fly.io Sprites, as the `warden` service on the Warden Sprite
```

## warden-call

Makes a single signed request through the Warden. It's the agent's only network tool, and the quickest way to test a mandate by hand.

```text
warden-call keygen                               new agent key and its address
warden-call address                              address of $MANDATE_AGENT_KEY
warden-call <METHOD> <path> [json-body]          a GitHub call
warden-call --ollama <METHOD> <path> [json-body] an Ollama call
```

It reads `MANDATE_AGENT_KEY` (hex) and `MANDATE_WARDEN_URL`. Paths are relative to the provider, so `repos/OWNER/REPO/issues` becomes `/v1/gateway/github/repos/OWNER/REPO/issues` on the Warden.

```bash
warden-call keygen
# MANDATE_AGENT_KEY=…   keep this on the agent Sprite only
# address=0x…           grant mandates to this address

warden-call GET repos/dicethedev/mandate-demo/issues
warden-call POST repos/dicethedev/mandate-demo/issues '{"title":"Found a bug"}'
warden-call --ollama POST api/chat \
  '{"model":"gpt-oss:120b","stream":false,"messages":[{"role":"user","content":"hi"}]}'
```

The HTTP status and the Warden's decision go to stderr and the response body to stdout, so you can pipe the body into `jq`. A denial looks like this:

```text
$ warden-call PUT repos/dicethedev/mandate-demo/contents/README.md '{"message":"edit","content":"…","branch":"main"}'
HTTP 403 · mandate: deny
{"decision":"deny","reason":"RECIPIENT_NOT_ALLOWED","message":"Target main is not one of the allowed: agent/work.", …}
```

**Exit codes:** `0` for a 2xx response, `2` for any other HTTP status (including a denial), `1` for a local error such as a missing key.

## mandate-agent

A short tool-calling loop. It sends the task to the model through the Warden, with one tool, `github_request`. It runs each tool call through the Warden and feeds the result back, including any denial and its reason.

```bash
mandate-agent "<task>"
```

| Variable | Default | Purpose |
|---|---|---|
| `MANDATE_AGENT_KEY` | required | The agent's signing key, from `warden-call keygen` |
| `MANDATE_WARDEN_URL` | required | The Warden's URL |
| `MANDATE_REPO` | `dicethedev/mandate-demo` | The repository the agent works on |
| `MANDATE_MODEL` | `gpt-oss:120b` | The Ollama model. It needs a `model.chat` mandate for this model |
| `MANDATE_MAX_STEPS` | `12` | Stop after this many model turns |

A real run on the agent Sprite, where the agent may only write to `agent/work`:

```text
$ mandate-agent 'Add a line "Maintained by an agent." to the end of README.md directly on the main branch.'
agent 0x818c…2a90 · model gpt-oss:120b · repo dicethedev/mandate-demo

✓ allow  GET repos/dicethedev/mandate-demo/contents/README.md?ref=main  (HTTP 200)
✗ DENY   PUT repos/dicethedev/mandate-demo/contents/README.md  (HTTP 403)
         Target main is not one of the allowed: agent/work.

The request to update README.md on the main branch was denied by the Mandate Warden:
- Reason: Target branch "main" is not among the allowed branches (only "agent/work" is permitted for writes).
- Mandate decision: RECIPIENT_NOT_ALLOWED.
```

The agent stops when the model answers without a tool call, or after `MANDATE_MAX_STEPS` turns. Its prompt tells it not to work around denials, but the prompt isn't the security boundary. The Warden is.

With Docker Compose, run it as `docker compose run --rm agent mandate-agent "<task>"`.

## Granting and revoking on a Warden

The Warden's owner key signs root mandates, so grants and revocations go through the Warden's admin API, not a local CLI:

```bash
# Let the agent write files on agent/work only
curl -X POST "$WARDEN/v1/admin/mandates" \
  -H "authorization: Bearer $WARDEN_ADMIN_TOKEN" -H 'content-type: application/json' \
  -d '{"subject":"0xAGENT","action":"contents.write","resource":"github:OWNER/REPO","allowedRecipients":["agent/work"]}'

# Revoke it: the agent's next write is denied with REVOKED
curl -X POST "$WARDEN/v1/admin/revoke" \
  -H "authorization: Bearer $WARDEN_ADMIN_TOKEN" -H 'content-type: application/json' \
  -d '{"mandateId":"0x…","reason":"demo over"}'
```

You can also grant, revoke, checkpoint and restore from the dashboard. See [Granting mandates](warden.md#granting-mandates) for model, read and issue mandates, and for checkpoints and restores.

## mandate

`mandate` runs the same authorization engine as the Warden, on your machine, with local development keys. Use it to work out a policy, including delegation and revocation, before granting it on a Warden. Action, resource and recipient names are the ones the Warden assigns to calls (see [How calls are classified](warden.md#how-calls-are-classified)): for GitHub, the resource is `github:OWNER/REPO` and the recipient of a write is its branch.

State lives in `./.mandate/` (override with `--home` or `MANDATE_HOME`).

```bash
mandate principal create owner
mandate principal create coding-agent
mandate principal create docs-agent

# The owner owns the repository, so only it can start a chain for it.
mandate resource own github:dicethedev/mandate-demo --owner owner

# The coding agent may write to two branches, and may hand on part of that.
mandate grant --from owner --to coding-agent \
  --action contents.write --resource github:dicethedev/mandate-demo \
  --recipient agent/work --recipient agent/docs --delegable --expires-in 7d

# It gives a docs sub-agent one branch, for one day.
mandate delegate --mandate M1 --from coding-agent --to docs-agent \
  --recipient agent/docs --expires-in 24h
```

```text
Delegation valid. Mandate M2 created

  owner
   └── coding-agent  [any amount → agent/work, agent/docs]
        └── docs-agent  [any amount → agent/docs]
```

Check what the sub-agent can do:

```bash
mandate check --agent docs-agent --action contents.write \
  --resource github:dicethedev/mandate-demo --recipient agent/docs
```

```text
✓ AUTHORIZED

  Agent        docs-agent
  Request      contents.write → agent/docs  on github:dicethedev/mandate-demo
  Authority    owner
  Delegation   owner → coding-agent → docs-agent
  Limits       any amount → agent/docs
  Mandate      M2
```

The same check with `--recipient main` gives `✗ DENIED  RECIPIENT_NOT_ALLOWED`. Delegating a branch the parent doesn't have is refused before anything is stored:

```text
$ mandate delegate --mandate M1 --from coding-agent --to docs-agent --recipient main --expires-in 24h
✗ DELEGATION REJECTED  RECIPIENT_ESCALATION

  child allows recipients the parent does not.

  Parent       M1  owner → coding-agent  [any amount → agent/work, agent/docs]  expires 2026-12-04 11:20 UTC
  Requested    coding-agent → docs-agent  [any amount → main]  expires 2026-11-28 11:20 UTC

  A delegated mandate may only narrow its parent's authority. Nothing was stored.
```

Revoking the coding agent's mandate cuts off everything delegated beneath it:

```bash
mandate revoke M1
mandate check --agent docs-agent --action contents.write \
  --resource github:dicethedev/mandate-demo --recipient agent/docs
mandate chain M2
```

```text
Mandate M1 revoked

  Revoked by   owner
  Reason       user_revoked
  Revocation   0xe17b5d8abd5b221e41fad7100124787f140964b8a94c00946ba8a8fa453ad8b1

  Also cut off (delegated beneath M1):
    M2  coding-agent → docs-agent  [any amount → agent/docs]

✗ DENIED  ANCESTOR_REVOKED

  Agent        docs-agent
  Request      contents.write → agent/docs  on github:dicethedev/mandate-demo
  Revoked      M1 by owner (an ancestor)
  Delegation   owner → coding-agent → docs-agent
  Mandate      M2

Chain for M2

  owner
   │  M1  contents.write any amount → agent/work, agent/docs  delegable  expires 2026-12-04 11:20 UTC
   │      ✗ revoked by owner (user_revoked)
   ▼
  coding-agent
   │  M2  contents.write any amount → agent/docs  expires 2026-11-28 11:20 UTC
   │      ✓ valid
   ▼
  docs-agent

  Result       ✗ chain broken: M1 is not usable, so M2 grants nothing
```

Amounts work the same way. `--max-amount` sets a limit, `check --amount` tests against it, and going over it gives `AMOUNT_EXCEEDED`. This is useful for resources such as budgets or payments.

Other commands: `mandate inspect M2`, `mandate principal list`, `mandate resource list` and `mandate export M1`. Mandates can be referred to as `M1`, by full ID, or by a unique ID prefix. **Exit codes:** `0` success or ALLOW, `1` DENY or rejected delegation, `2` error, so `mandate check` can gate a shell script.

> [!WARNING]
> `.mandate/keys.json` holds **plaintext development private keys** (written owner-only). It's for local testing only and is git-ignored. Never use these keys for anything real.

**From `mandate` to a Warden.** `mandate export M1` prints the exact signed document, and a Warden accepts it at `POST /v1/mandates` as `{"mandate": <document>}`. It only grants something there if its chain leads back to that Warden's owner key. Mandates signed with local development keys never do, so on a Warden, grant through the [admin API](#granting-and-revoking-on-a-warden).
