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
