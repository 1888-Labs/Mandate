# Security

[← Back to the README](../README.md)

## Security model

Mandate v0.1 is designed to reject each of the following. ✅ means it's implemented and covered by tests today.

| Attack | Defence | Status |
|---|---|---|
| Forged mandate | Issuer signature must recover to `issuer` | ✅ |
| Storage tampers with a mandate | ID and signature are recomputed from content | ✅ |
| Privilege escalation through delegation | `can_delegate` attenuation rules | ✅ |
| Self-issued authority (agent grants itself a root mandate) | Root issuer must be a trusted owner of the resource | ✅ |
| Wrong resource or action | Exact match required along the chain | ✅ |
| Expired authority | Signed `expires_at` of every link checked by the engine, not storage | ✅ |
| Amount above the delegated limit | Effective constraints of the chain | ✅ |
| Recipient not allowed | Effective constraints of the chain | ✅ |
| Forged record planted next to a real one | Every candidate verified; closest real reason reported | ✅ |
| Revoked authority | Signed revocation documents | ✅ |
| Ancestor revoked | Full-chain check on every request | ✅ |
| Forged or replayed revocation | Must target the exact mandate, be signed by its issuer, and use the revocation domain separator | ✅ |
| Forged copy of a real mandate published to break a chain | Store returns every copy; the engine uses the one that verifies | ✅ |
| Operator deletes a revocation to revive a mandate | Revocation entities are handed to a burn address in the creating transaction; not even the issuer can delete them | ✅ (verified on Tiramisu) |
| Someone else publishes records "from" Alice | Only entities whose on-chain `$creator` is the document's issuer are used, checked in the Arkiv query and again in Rust | ✅ (verified on Tiramisu) |
| Flood of fake revocations to hide the real one | Revocations are queried with `$creator = issuer`; any other lookup over the result cap fails closed | ✅ |

## Threat model

Mandate enforces authorization **when the protected tool honours Mandate's decision**. It does **not** protect against:

- a compromised root private key
- a service or application that ignores Mandate and calls the tool directly
- confidentiality of anything deliberately published to Arkiv (all records are public)
- a malicious operating system or host
- bugs inside the protected external tool
- an LLM being wrong, or every possible prompt-injection attack
- **quantum adversaries:** mandates use secp256k1 ECDSA, like Ethereum itself. A large enough quantum computer could forge these signatures. Post-quantum signatures are out of scope for v0.1.
