# Security Policy

## Supported versions

Mandate is experimental software built for the Fly.io Hacksgiving 2026 hackathon. There is no long-term support guarantee yet. Security fixes will be applied to the `main` branch.

## Reporting a vulnerability

If you believe you have found a security issue in Mandate (the authorization engine, the Warden, the signing scheme, or the isolation model), please report it privately.

**Preferred:** open a private security advisory on the GitHub repository, or email the maintainers if contact information is listed on the organization profile.

Please include:

- A clear description of the issue and its impact
- Steps to reproduce (or a proof-of-concept)
- Affected components (`mandate-core`, `mandate-crypto`, `mandate-warden`, classifier, etc.)
- Whether you believe the issue is exploitable by a compromised agent, a malicious storage backend, or an external attacker

We will acknowledge the report as quickly as we can and keep you informed of progress.

## Scope

Mandate is designed to enforce signed, attenuating, revocable authority on the path that goes through the Warden. The threat model and the concrete defences are documented in detail in [docs/security.md](docs/security.md).

In short, Mandate aims to stop:

- forged or escalated mandates
- replayed or unsigned requests
- an agent acting outside the authority the owner actually granted
- privilege escalation through delegation

It does **not** claim to protect against a fully compromised owner key, a misconfigured network policy that lets the agent bypass the Warden, or every possible prompt-injection attack against the model. Those limits are intentional and documented.

## Out of scope

- Issues that only affect the demo / example agent behaviour when the Warden correctly denied the call
- Theoretical quantum attacks against secp256k1 (the same primitive Ethereum uses)
- Social-engineering of the human owner

Thank you for helping keep Mandate safe.
