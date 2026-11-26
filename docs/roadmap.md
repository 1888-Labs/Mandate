# Roadmap

[← Back to the README](../README.md)

Where Mandate is going. These are plans, not promises: priorities shift with what people building agents actually need. To push something up the list, or propose something new, [open an issue](https://github.com/1888-Labs/Mandate/issues).

## Where it stands

The core is complete. It has signed mandates, delegation that can only narrow authority, revocation that cascades to everything beneath it, and explained ALLOW or DENY decisions. You can use it through a Rust library and a CLI.

On `main`, Mandate runs as a gateway on [Fly.io Sprites](https://fly.io/sprites). The Warden fronts the GitHub and Ollama connectors for an agent on a separate, egress-locked Sprite. It checks every signed call, checkpoints the agent before writes, and can roll it back after a denied one. It hasn't had an external audit, so it's ready for experiments and personal agents, not for production systems.

## Next

- **A live dashboard** on the Warden URL: decisions as they happen, the mandate tree, and one-click revoke and restore.
- **More connectors** behind the same gateway, each with its own classifier, so one mandate model covers every tool an agent uses.
- **Budgets, not just per-call checks.** Limits across many calls, such as model tokens per day, issues per hour or writes per branch.
- **Delegation from the agent.** Let an agent hand a narrower mandate to a sub-agent on its own Sprite. The Warden already accepts signed delegated mandates at `POST /v1/mandates`.
- **Admin keys instead of a bearer token.** Owner-signed admin requests, so revoking doesn't depend on a shared secret.
- **External security audit** of `mandate-core`, `mandate-crypto` and the Warden's signing and classification.

## Later

- **MCP integration.** A Mandate-aware MCP proxy that checks every tool call before it reaches the server.
- **Signed receipts.** An owner-verifiable record of each ALLOW, for audits and incident reviews.
- **Packages:** `mandate-core`, `mandate-crypto` and `mandate-cli` on crates.io, and a prebuilt Warden image.

## Not planned

Mandate is an authorization layer. It will stay out of tokens, NFTs, DAO governance, reputation scores, custom chains, and moving money itself.
