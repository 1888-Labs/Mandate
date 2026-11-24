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

An agent runs on its own Sprite with no keys and no connector access. Its only way out is the **Mandate Warden**, a second Sprite that alone may use the Sprites connectors. Every call the agent makes is signed with the agent's key, checked against signed, scoped, revocable mandates, and only then forwarded through the connector. Denials come back with a reason (`RECIPIENT_NOT_ALLOWED`, `NO_MANDATE`, `REVOKED`, …).

| Crate | What it is |
|---|---|
| [`mandate-core`](crates/mandate-core) | Mandates, delegation and the authorization engine |
| [`mandate-crypto`](crates/mandate-crypto) | EIP-191 signing and verification |
| [`mandate-warden`](crates/mandate-warden) | The gateway in front of Sprites connectors, plus `warden-call`, the agent's signed client |
| [`mandate-cli`](crates/mandate-cli) | Issue, delegate and check mandates from the terminal |

Full write-up, demo and setup guide coming during the week.
