# Roadmap

[← Back to the README](../README.md)

Where Mandate is going after v0.1. These are plans, not promises: priorities shift with what people building agents actually need. To push something up the list, or propose something new, [open an issue](https://github.com/1888-Labs/Mandate/issues).

## Where it stands

v0.1 is a complete, working system. It has signed mandates, delegation that can only narrow authority, revocation that cascades to everything beneath it, and explained ALLOW or DENY decisions. Shared state lives on Arkiv. You can use it through a Rust library, a CLI, an HTTP API, a web UI and a [Docker image](self-hosting.md). It runs on Arkiv's Tiramisu **testnet** and hasn't had an external audit, so it's ready for prototypes and pilots, not yet for real money or production systems.

## Next: ready for production

The goal is that a team can protect real tools with Mandate and trust the result.

- **External security audit** of `mandate-core` and `mandate-crypto`: delegation, signatures, chain verification and revocation.
- **Arkiv mainnet**, as soon as it's available, with the network chosen in configuration rather than built in.
- **Publishing without handing over keys.** Today a server can only publish for issuers whose keys it holds. Issuers should be able to publish from their own wallet (browser wallet, hardware key or KMS), so the server never needs anyone's signing key.
- **Client SDKs for TypeScript and Python**: authorize, sign and publish in a few lines, with fail-closed defaults built in.
- **Packages:** `mandate-core`, `mandate-crypto` and `mandate-cli` on crates.io, so `cargo install mandate-cli` just works.
- **API access control.** Reads and `authorize` are open today. Optional API keys or mTLS, plus rate limiting, for deployments exposed beyond a private network.
- **Operations:** Prometheus metrics (decisions, deny reasons, Arkiv latency), structured JSON logs, and a Helm chart.

## Later: richer authority

- **MCP integration.** A Mandate-aware MCP proxy that checks every tool call before it reaches the server, so each connected agent can only use what it was granted, with no changes to the tools themselves.
- **Budgets, not just per-call limits.** Constraints across many requests, such as "$1,000 per day in total" or "10 deployments per week", plus time-of-day windows.
- **More constraint types** beyond amount and recipient, for common actions like repository access, environments and data scopes.
- **Signed authorization receipts.** Optionally publish each ALLOW as an attestation on Arkiv, giving an independent, tamper-proof audit trail of what agents actually did.
- **Historical checks:** "Was this agent allowed to do this at 14:02 last Tuesday?", answered from Arkiv's history for audits and incident reviews.
- **Broader revocation.** Let an ancestor's issuer cut off any branch beneath it directly, and support emergency revocation by a designated guardian.

## Exploring

Ideas we think are promising but haven't designed yet:

- A small policy language for conditions that don't fit fixed constraint fields.
- Multi-signature root mandates, so granting authority over a treasury takes two people.
- Post-quantum signatures, once there's a standard that fits Ethereum-style tooling.
- Other storage backends next to Arkiv, behind the existing `MandateStore` trait.

## Not planned

Mandate is an authorization layer. It will stay out of tokens, NFTs, DAO governance, reputation scores, custom chains, and moving money itself.
