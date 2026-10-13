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


---

## 2026-10-13 — Trust anchors: who may issue a root mandate (security fix)

**Decision**
`authorize` takes a `TrustAnchors` map of resource → principals allowed to issue *root* mandates for that resource. A chain whose root issuer is not trusted for its resource is denied with the new reason `UNTRUSTED_ROOT`.

**Reason**
Neither spec says who may start a chain. Without this, any agent could sign a root mandate to itself ("InvoiceAgent → InvoiceAgent: transfer anything to anyone"). The engine would find a validly signed mandate and return ALLOW, making every limit in the demo bypassable. A signature only proves *who* granted authority, not that they *had* it to grant.

**Alternatives considered**
- Pass the expected root principal in each `AuthorizationRequest`. Rejected: it puts the caller in charge of a security decision on every call.
- Store resource ownership on Arkiv. Possible later, but it needs its own ownership-proof design and is out of scope for v0.1.

**Consequences**
The CLI and server must be configured with resource owners (for the demo: Alice owns `wallet:treasury-demo`). `DenyReason` has one variant beyond the spec's list. The README security table lists this protection.

---

## 2026-10-13 — Denials carry structured detail

**Decision**
`AuthorizationDecision::Deny` holds a `Denial { reason, mandate_id, delegation_path }` rather than a bare `DenyReason`. `DenyReason` variants carry their evidence, for example `AmountExceeded { requested, maximum }`, `Expired { mandate_id, expires_at }`, `InvalidDelegation { mandate_id, fault }`. `code()` gives the stable string (`AMOUNT_EXCEEDED`, …).

**Reason**
The spec requires every failure to be explainable, and its sample outputs show requested vs maximum, the delegation path and the revoked ancestor. A bare enum can't carry that.

**Consequences**
The API maps `Denial` to the spec's JSON shape (`reason`, `requestedAmount`, `maximumAmount`, …) in Milestone 10.

---

## 2026-10-13 — Authorization engine semantics

**Decision**
- **Boundary:** the engine depends on two traits defined in `mandate-core`: `MandateStore` (read state) and `SignatureVerifier` (ID integrity plus signature). `mandate-crypto::EthereumVerifier` implements the verifier, which keeps core free of cryptography. `MandateStore` gains `find_revocation` in Milestone 6, and Milestone 7 adds `MemoryMandateStore`.
- **Order of checks per candidate:** action/resource match → resolve and verify every mandate leaf→root → every edge passes `can_delegate` → root trusted → no mandate expired → effective constraints. Revocation goes between expiry and constraints in Milestone 6.
- **Expiry:** a mandate is valid while `timestamp < expires_at`. At `timestamp == expires_at` it is expired. `issued_at` is not checked (no not-before semantics in v0.1). The engine reports the highest expired mandate in the chain.
- **Effective constraints:** the minimum amount and the intersection of recipient sets over the whole chain. For a valid chain this equals the leaf's, but computing it from every link keeps it correct regardless.
- **Fail closed on missing request fields:** if a limit applies and the request has no `amount`, the result is `AMOUNT_EXCEEDED` (`requested: null`). If recipients are restricted and there is no `recipient`, the result is `RECIPIENT_NOT_ALLOWED`.
- **Several candidates:** candidates are evaluated in mandate-ID order, and any one that fully authorizes gives ALLOW. If all are denied, the denial from the candidate that got furthest is reported (constraints > revocation > expiry > untrusted root > invalid delegation > bad signature > action/resource). A planted forgery or self-issued mandate therefore can't hide the real reason (`AMOUNT_EXCEEDED`).
- **Depth limit:** at most `MAX_CHAIN_DEPTH = 16` delegation hops. A deeper chain is `INVALID_DELEGATION` / `TooDeep`.
- **Untrusted storage:** mandates for a different subject are ignored even if the store returns them. A missing parent is `INVALID_DELEGATION` / `MissingParent`.

**Consequences**
Tests cover each rule. A mutation check confirmed the suite fails if signature checking, `can_delegate`, the trust-anchor check or the expiry boundary is removed or weakened. `amount_above_effective_limit_is_denied` (BUILD_SPEC §34) is implemented as `amount_above_limit_is_denied`.

---

## 2026-10-13 — Revocation semantics

**Decision**
- **Format:** `Revocation` is a separate signed document with its own `RevocationId` newtype (BUILD_SPEC §14). The ID is `keccak256` of a canonical encoding with its own domain separator, `mandate-revocation/0.1`. The issuer signs it with EIP-191 like a mandate. Because the domain separators differ, a mandate signature can never be passed off as a revocation signature, or the reverse (tested).
- **Who may revoke:** only the mandate's own issuer, as the spec says for v0.1. Upstream principals cut off a descendant by revoking their *own* mandate, and that invalidates everything beneath it.
- **When it applies:** a valid revocation applies as soon as the engine can see it, whatever its `revoked_at`. `revoked_at` is informational, for audit. Reason: if `revoked_at` were enforced as "effective from", clock skew between the revoker and the server could leave a just-revoked mandate usable.
- **Engine order:** revocation is checked after expiry and before constraints, over every mandate in the chain, top first. A revoked leaf gives `REVOKED`. A revoked ancestor gives `ANCESTOR_REVOKED`, naming the *highest* revoked mandate. Both carry the `revocation_id`, so the record can be looked up independently.
- **What counts:** a revocation counts only if it targets this mandate's ID, its issuer is that mandate's issuer, and its ID and signature verify. Everything else the store returns is ignored.

**Consequences**
Revocation is permanent in v0.1. There is no un-revoke; the issuer grants a new mandate instead.

---

## 2026-10-13 — `MandateStore::find_revocations` returns all revocations (security)

**Decision**
The spec's trait has `find_revocation(&MandateId) -> Option<Revocation>`. Mandate uses `find_revocations(&MandateId) -> Vec<Revocation>`, and the engine checks every result.

**Reason**
Storage is untrusted, and anyone can publish a record that *claims* to revoke a mandate. With a single `Option`, an attacker who wants a revoked mandate to keep working could publish a forged revocation. If the store returned the forgery instead of the real one, the engine would discard it and treat the mandate as live. Returning all candidates means a forgery can't hide a real revocation (`forged_revocation_cannot_hide_real_one`).

**Consequences**
The Arkiv adapter (Milestone 9) must return every matching revocation entity, not just the first.

---

## 2026-10-13 — Revocation test names

The specs use several names for the same revocation tests. Each is implemented once:

| Implemented as | Also named in the specs |
|---|---|
| `directly_revoked_mandate_is_denied` | `direct_revocation_is_respected` (AGENTS test list), `directly_revoked_mandate_is_rejected` (BUILD_SPEC §34) |
| `revoked_parent_invalidates_child` | `parent_revocation_invalidates_child` (AGENTS test list) |
| `revoked_grandparent_invalidates_descendant` | `grandparent_revocation_invalidates_descendant` (AGENTS test list) |

---

## 2026-10-13 — `MandateStore::get_mandates` returns every copy of an ID (security)

**Decision**
The spec's trait has `get_mandate(&MandateId) -> Option<Mandate>`. Mandate uses `get_mandates(&MandateId) -> Vec<Mandate>`. When resolving a parent, the engine ignores copies whose ID doesn't match, prefers a copy whose signature verifies, and reports `INVALID_SIGNATURE` only if no copy verifies.

**Reason**
A mandate ID covers the content but not the signature, so anyone can publish a copy of a real mandate with the same ID and a garbage signature. With a single `Option`, a store that returned that copy would break every chain running through the real mandate: a denial-of-service anyone could trigger. This is the same reasoning as for `find_revocations`.

**Consequences**
- The Arkiv adapter must return every entity with a matching `mandate_id`.
- A test makes the forged copy sort *before* the real one, so the engine has to actively prefer the copy that verifies. An earlier version of the test passed by ordering luck, and a mutation check caught it.

---

## 2026-10-13 — `MemoryMandateStore`

**Decision**
`mandate-core` provides `MemoryMandateStore`: an append-only `MandateStore` that, like Arkiv, accepts any record and verifies nothing. It ignores exact duplicates (same ID and signature) and keeps every other record, including different copies under one ID. It also offers `find_children(parent)` for chain display.

**Reason**
The engine must be proven against a store that behaves like the real untrusted one, before Arkiv is integrated (AGENTS.md Milestone 7).

**Consequences**
Lookups are linear scans, which is fine at demo scale. The Milestone 7 acceptance test (`crates/mandate-crypto/tests/authorization_flow.rs`) runs all seven demo steps against it with real signatures and planted forgeries.

---

## 2026-10-13 — CLI design

**Decision**
- **Local state until Arkiv exists.** The CLI keeps state in a directory (`.mandate/` by default; `--home` / `MANDATE_HOME`). `state.json` holds only public data: principal names and addresses, resource owners, signed mandates, revocations, and aliases. `keys.json` holds development private keys, written owner-only (`0600`, directory `0700`). Both files are written atomically (temp file + rename). Every command rebuilds a `MemoryMandateStore` from `state.json` and calls the same `authorize` the server will use. Nothing in the file is trusted: a hand-edited mandate fails signature verification (tested).
- **`mandate resource own <resource> --owner <name>`** is a command the spec doesn't list. It's needed because of trust anchors: someone has to say who may start a chain. `grant` refuses non-owners up front, and the error message names the fix.
- **Aliases.** Mandates get `M1`, `M2`, … in creation order, matching the spec's CLI examples. Full IDs and unique prefixes (≥ 6 chars) also work.
- **`delegate` defaults.** If a flag is omitted, the child inherits it from the parent: action, resource, limit, recipients and expiry. That matches the spec example, where `--recipient` is omitted and the child stays limited to AWS. Before saving, `delegate` runs `can_delegate` and refuses a parent that is expired, revoked, badly signed or missing a parent. A rejected delegation prints the error code and stores nothing.
- **Exit codes.** `0` success / ALLOW, `1` DENY or rejected delegation, `2` error. Scripts can use `mandate check` as a gate.
- **Display.** Amounts print as `$N` to match the spec's demo output. The protocol itself still treats amounts as plain integers. `inspect` and `chain` judge each link with the same helpers as the engine (`find_mandate`, `find_valid_revocation`, `can_delegate`), not a reimplementation.

**Consequences**
Milestone 9 replaces the local `state.json` store with Arkiv. Keys stay local and never go to Arkiv.

---

## 2026-10-13 — Arkiv adapter design

**Decision**
- **Boundary:** a TypeScript HTTP service (`apps/arkiv-adapter`) that stores and queries signed documents and never interprets them. All semantics stay in Rust.
- **Byte-exact payloads:** the adapter stores the request body as received and returns stored text verbatim. CLI nonces exceed 2⁵³, so any JS parse-and-stringify would change them and break IDs and signatures.
- **Revocations can't be deleted:** they are created and transferred to `0x…dEaD` in one `executeBatch`. Arkiv's `readonly` flag blocks patches but not deletion by the owner, so without this the operator could delete a revocation and revive a revoked mandate. Verified on Tiramisu.
- **Lifetimes:** mandate entities expire at the signed `expiresAt` (`ExpirationTime.atDate`). Revocations live until the revoked mandate's expiry plus 1 h. Both are `readonly`.
- **Deviations from the spec's attribute list:** `policy_hash` is omitted, because the spec doesn't define it and nothing queries it. Added: `version`, plus `expires_at` (u64) for time filtering. `parent_id` is omitted for root grants. `depth` comes from storage and is informational only. `version` is an attribute, not added to the payload, so the payload stays exactly the signed document.
- **Writes** can require a bearer token (`ADAPTER_TOKEN`), since they spend the adapter wallet's gas.
- **Acceptance tests** write to `<project_id>-acceptance`, keeping the demo namespace clean.

**Consequences**
Mandate entities are owned by the adapter wallet and can be deleted by it. That can only cause denials (a missing parent), never grants. Documented in `arkiv/schema.md`.
