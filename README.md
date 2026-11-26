<br/>

<p align="center">
  <a href="https://github.com/1888-Labs/Mandate">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset=".github/logo-dark.svg">
      <img alt="Mandate" src=".github/logo-light.svg" width="auto" height="80">
    </picture>
  </a>
</p>

<p align="center">
  A personal agent on a leash: verifiable, revocable authority for AI agents on <a href="https://fly.io/sprites">Fly.io Sprites</a>.
</p>

<br/>

> [!NOTE]
> `main` is being rebuilt for the Fly.io **Hacksgiving** hackathon (Nov 23–29, 2026). The previous Arkiv version lives on the [`arkiv`](https://github.com/1888-Labs/Mandate/tree/arkiv) branch.

**The prompt is not the security boundary. Mandate is.**

A personal coding agent runs on its own Sprite. It has no GitHub token, no model API key, and a network policy that only lets it reach one place: the **Mandate Warden**, a second Sprite and the only one allowed to use the Sprites connectors. Every call the agent makes, both to *think* (Ollama) and to *act* (GitHub), is signed with the agent's key. The Warden checks each one against signed, scoped, revocable mandates before forwarding it. Denials come back with a reason (`RECIPIENT_NOT_ALLOWED`, `NO_MANDATE`, `REVOKED`, …). If the agent tries a write it isn't allowed, the Warden can roll its Sprite back to the last known-good checkpoint.

```text
mandate-agent Sprite ──signed call──▶ Mandate Warden Sprite ──▶ GitHub connector
  (egress: Warden only)               verify · classify ·   ──▶ Ollama connector
                                      authorize · log      ──▶ Sprites API (checkpoint / restore)
```

## What it looks like

A real run on the agent Sprite. The owner has granted the agent `model.chat` on `gpt-oss:120b`, read access to the repository, and `contents.write` only on the `agent/work` branch. Asked to edit `main` directly:

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

The model call that planned this went through the Warden too, checked against the `model.chat` mandate. Revoke the mandate from the Warden, and the next call is denied with `REVOKED`.

## Crates

| Crate | What it is |
|---|---|
| [`mandate-core`](crates/mandate-core) | Mandates, delegation and the authorization engine |
| [`mandate-crypto`](crates/mandate-crypto) | EIP-191 signing and verification |
| [`mandate-warden`](crates/mandate-warden) | The Warden gateway in front of the Sprites connectors, plus two agent-side binaries: `warden-call` (one signed call) and `mandate-agent` (the agent) |
| [`mandate-cli`](crates/mandate-cli) | Issue, delegate and check mandates from the terminal |

## Docs

- [Architecture](docs/architecture.md): two Sprites, and how each call is decided
- [The Warden](docs/warden.md): HTTP API, request signing, classification, configuration, and setting up two Sprites
- [Protocol](docs/protocol.md): mandates, delegation, revocation and deny codes
- [Security](docs/security.md): what's defended and what isn't
- [CLI](docs/cli.md) and [library](docs/library.md)
- [Development](docs/development.md), [roadmap](docs/roadmap.md) and [inspiration](docs/inspiration.md)

## License

MIT
