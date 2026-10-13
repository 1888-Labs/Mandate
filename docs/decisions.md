# Decisions

Material deviations from, or clarifications of, `AGENTS.md` and `BUILD_SPEC.md`.

---

## 2026-10-12 — Signature field is `Vec<u8>`, serialized as `0x` hex

**Decision**
`Mandate.signature` is `Vec<u8>` (as in `AGENTS.md`), not a dedicated `Signature` type (as in `BUILD_SPEC.md` §7). In JSON it is a `0x`-prefixed hex string, matching the Arkiv payload example.

**Reason**
`mandate-core` must not depend on a crypto library. Signature parsing and recovery belong in `mandate-crypto` (Milestone 4).

**Alternatives considered**
A `Signature` newtype in `mandate-core`. It could be added later without changing the wire format.

**Consequences**
`mandate-core` treats signatures as opaque bytes. Malformed hex is rejected at deserialization time.

---

## 2026-10-12 — Resource kind is everything before the first `:`

**Decision**
`ResourceId` is parsed from `kind:id` by splitting on the first `:`. `mcp:github:create_issue` → kind `mcp`, id `github:create_issue`.

**Reason**
The spec's resource examples have two or three segments. Splitting on the first colon handles all of them without inventing a hierarchy.

**Alternatives considered**
A multi-segment path type. Rejected as premature for v0.1, which only compares resources for exact equality.

**Consequences**
Resource comparison is exact equality on `(kind, id)`. There is no wildcard or prefix matching.

---

## 2026-10-12 — JSON field names are camelCase

**Decision**
Protocol types serialize with camelCase field names (`maxAmount`, `issuedAt`, ...), matching the Arkiv payload examples in both specs.

**Reason**
One wire format for the API, the CLI and Arkiv payloads.

**Consequences**
This JSON is a convenience format only. Mandate IDs will be computed from a separate, explicit canonical encoding (Milestone 2), never from JSON.

---

## 2026-10-12 — Canonical mandate encoding

**Decision**
Mandate IDs are `keccak256` of a hand-written binary encoding of `UnsignedMandate` (`crates/mandate-core/src/encoding.rs`). Every string and list is prefixed with a u64 big-endian length, `Option` gets a `0x00`/`0x01` tag, and the bytes start with the domain separator `mandate/0.1`. IDs are written as `0x` + 64 lowercase hex characters.

**Reason**
The spec forbids relying on JSON field ordering. Length prefixes make the encoding unambiguous: shifting bytes across a field boundary cannot produce the same output. The domain separator stops a mandate hash from colliding with any other hash the system produces.

**Alternatives considered**
RLP / ABI encoding (adds a dependency, harder to read). Canonical JSON (RFC 8785) (needs a careful implementation in both Rust and TS).

**Consequences**
A unit test pins the exact bytes for a sample mandate. Any change to the format breaks that test on purpose, because it would invalidate every existing ID. If the format ever changes, bump the domain separator.

---

## 2026-10-12 — `allowed_recipients` is a set

**Decision**
The canonical encoding sorts and deduplicates `allowed_recipients`, so `[AWS, GitHub]`, `[GitHub, AWS]` and `[AWS, GitHub, AWS]` produce the same mandate ID.

**Reason**
Recipient checks (`child ⊆ parent`, `recipient ∈ allowed`) are set operations, and order carries no authority.

**Consequences**
Two `Mandate` structs that differ only in recipient order are unequal under Rust `==` but have the same ID. Authorization logic must compare recipients as sets, never as `Vec` equality. Recipient names are case-sensitive (`AWS` ≠ `aws`).

---

## 2026-10-13 — Principals are lowercase addresses (resolves the 2026-10-12 open item)

**Decision**
A principal's canonical form is `0x` + 40 lowercase hex digits. `mandate-crypto` derives addresses in this form and `parse_address` lowercases EIP-55 input. `sign_mandate` and `verify_mandate_signature` reject any mandate whose `issuer` or `subject` is not canonical (`NON_CANONICAL_ADDRESS`).

**Reason**
EVM addresses ignore case, but the mandate ID and every equality check (`child.issuer == parent.subject`) compare exact strings. Allowing more than one spelling of an address would let the same principal appear under two IDs.

**Alternatives considered**
Normalizing at comparison time. Rejected: the mandate ID would still depend on the spelling, and every comparison site would have to remember to normalize.

**Consequences**
`mandate-core` stays crypto-free and compares exact strings. The canonical-form rule is enforced at the signature boundary, which every mandate must pass through before it is trusted.
---

## 2026-10-12 — `can_delegate` checks the parent–child edge only

**Decision**
`can_delegate(parent, child)` enforces exactly the rules in the spec: the parent is delegable; the child points at the parent and is issued by the parent's subject; the action and resource are equal; the amount limit and recipient set only narrow; the expiry is not later. It does **not** check signatures, ID integrity, expiry against the current time, or revocation.

**Reason**
Those are properties of each mandate, not of the edge. The authorization engine (Milestones 4–6) checks them for every mandate in the chain. Keeping `can_delegate` pure makes the core rule easy to read and to test.

**Consequences**
- No caller may treat `can_delegate` returning `Ok` as "this child is valid". The chain verifier must also check `id == compute_mandate_id(unsigned)` and the signature for every link.
- A parent with no limit of some kind (`None`) allows any child value for it. A parent with a limit (`Some`) requires the child to keep one that is equal or smaller. Dropping a restriction is an escalation.
- `Some([])` (no recipients allowed) is a valid narrowing of any recipient set.
- The child's `issued_at` is not compared with the parent's. Neither spec requires it, and backdating a child grants nothing because validity is bounded by `expires_at` and the parent's revocation. Revisit if issued-at / not-before semantics are added.
- When several rules fail, the first failing check is reported, in this order: delegable, parent link, issuer, action, resource, amount, recipients, expiry.

---

## 2026-10-12 — One name per required test

**Decision**
The two spec files name some required tests differently. Each test is implemented once, under the `AGENTS.md` Milestone 3 name:

| Implemented as | Also named in the specs |
|---|---|
| `child_can_reduce_recipients` | `child_can_remove_recipient` (BUILD_SPEC §34) |
| `child_issuer_must_match_parent_subject` | `child_issuer_matches_parent_subject` (AGENTS test list) |
| `non_delegable_parent_cannot_delegate` | `non_delegable_mandate_cannot_be_delegated` (BUILD_SPEC §34) |

**Reason**
Two identical tests under different names add maintenance cost and no coverage.

---

## 2026-10-13 — Signature scheme: EIP-191 `personal_sign` over the mandate digest

**Decision**
The issuer signs the 32-byte mandate digest (`keccak256(canonical_encoding)`) with EIP-191 `personal_sign`, using secp256k1 via the RustCrypto `k256` crate. Signatures are 65 bytes `r || s || v`, with `v` ∈ {27, 28}. Verification also accepts `v` ∈ {0, 1}, and rejects high-`s` signatures and any mandate whose stored `id` does not match its content.

**Reason**
- Ethereum-compatible as required: any wallet (MetaMask, Foundry `cast wallet sign`, ethers/viem `signMessage`) can sign a mandate by `personal_sign`-ing its ID bytes. A test pins a signature produced by Foundry 1.6.0.
- The EIP-191 prefix stops a mandate signature from also being a valid transaction signature.
- The verifier recomputes the digest from the content and never trusts the stored ID. Storage (Arkiv) therefore cannot change a mandate without breaking its signature or its ID.

**Alternatives considered**
EIP-712 typed data. Wallets would show it more readably, but it would mean keeping a second, parallel encoding of the mandate in sync with the canonical one. That can be revisited after v0.1.

**Consequences**
A wallet prompt shows only an opaque 32-byte hex value, not the mandate's fields. Low-`s` enforcement means each signer has exactly one valid signature per mandate. The signature is outside the ID either way.

