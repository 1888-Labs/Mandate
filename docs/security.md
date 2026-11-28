# Security

[← Back to the README](../README.md)

## Security model

Mandate is designed to reject each of the following. ✅ means it's implemented and covered by tests today.

### Mandates (`mandate-core`, `mandate-crypto`)

| Attack | Defence | Status |
|---|---|---|
| Forged mandate | Issuer signature must recover to `issuer` | ✅ |
| Storage tampers with a mandate | ID and signature are recomputed from content | ✅ |
| Privilege escalation through delegation | `can_delegate` attenuation rules | ✅ |
| Self-issued authority (agent grants itself a root mandate) | Root issuer must be a trusted owner of the resource; on the Warden that is only the owner key | ✅ |
| Wrong resource or action | Exact match required along the chain | ✅ |
| Expired authority | Signed `expires_at` of every link | ✅ |
| Amount above the delegated limit | Effective constraints of the chain | ✅ |
| Recipient not allowed (wrong branch, wrong model) | Effective constraints of the chain | ✅ |
| Forged record planted next to a real one | Every candidate verified; closest real reason reported | ✅ |
| Revoked authority | Signed revocation documents | ✅ |
| Ancestor revoked | Full-chain check on every request | ✅ |
| Forged or replayed revocation | Must target the exact mandate, be signed by its issuer, and use the revocation domain separator | ✅ |

### The Warden (`mandate-warden`)

| Attack | Defence | Status |
|---|---|---|
| Agent claims to be a different agent | The subject is the recovered signer; a mismatched `x-mandate-agent` is rejected | ✅ |
| Captured request replayed | Timestamps older or newer than 60 s are rejected; each signature is accepted once | ✅ |
| Request body swapped after signing | The body hash is part of the signed digest | ✅ |
| Call disguised as something harmless | Unrecognised calls get `other.*` / `account.write` actions no mandate grants | ✅ |
| Write to a protected branch | Branch is the recipient for content, ref and pull-request writes; `allowedRecipients` limits it | ✅ |
| Agent switches to a different model | Model is the recipient for Ollama calls | ✅ |
| Agent calls GitHub or Ollama directly | Agent Sprite's network policy only allows the Warden; connectors' access policy only allows the Warden Sprite | Platform config |
| Agent leaks or misuses a token | The agent holds no tokens; the Warden holds none either, the connector attaches them | Platform design |
| Agent tampers with its own environment after a bad action | Checkpoint before each allowed write; restore to known-good after a denied write | ✅ (opt-in) |

## Threat model

Mandate enforces authority on the path that goes through the Warden. It does **not** protect against:

- a compromised owner key or Warden Sprite
- a misconfigured network or connector policy that lets the agent bypass the Warden
- a malicious host or platform
- bugs in GitHub, Ollama or the connectors
- an LLM being wrong, or every possible prompt-injection attack. Mandate limits what a misled agent can *do*; it doesn't stop it from being misled
- harm done by calls that are allowed: a mandate for `issues.create` lets the agent open any issue on that repository
- **quantum adversaries:** mandates and requests use secp256k1 ECDSA, like Ethereum itself. A large enough quantum computer could forge these signatures.

Admin routes accept a signature from the owner key (`mandate-admin/1`, same skew and replay rules as agent calls). `Authorization: Bearer $WARDEN_ADMIN_TOKEN` still works so the dashboard can grant and revoke; it is a shared secret and a leak of it is enough to change authority. `GET /v1/state` and `POST /v1/mandates` are open by design: state contains no secrets, and submitted mandates grant nothing unless their chain leads back to the owner.
