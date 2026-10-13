// Shapes of the signed documents the Rust engine produces, and the Arkiv
// attributes derived from them.
//
// The adapter checks only that a document is well-formed enough to index.
// It never verifies signatures or interprets authority: the Rust engine
// re-verifies everything it reads back.

import { addr, bool, bytes32, i32, str, u64 } from "@arkiv-network/sdk/attr"
import { concat, type Hex, keccak256, numberToHex, stringToBytes, toHex } from "viem"

export const PROTOCOL_VERSION = "mandate/0.1"

export type Resource = { kind: string; id: string }

/** A signed mandate, exactly as `mandate-core` serializes it. */
export type Mandate = {
  id: Hex
  issuer: Hex
  subject: Hex
  action: string
  resource: Resource
  constraints: { maxAmount: number | null; allowedRecipients: string[] | null }
  parent: Hex | null
  delegable: boolean
  issuedAt: number
  expiresAt: number
  nonce: number
  signature: Hex
}

/** A signed revocation, exactly as `mandate-core` serializes it. */
export type Revocation = {
  id: Hex
  mandateId: Hex
  issuer: Hex
  revokedAt: number
  reason: string | null
  signature: Hex
}

export class InvalidDocument extends Error {}

const HASH = /^0x[0-9a-f]{64}$/
const ADDRESS = /^0x[0-9a-f]{40}$/
const HEX = /^0x([0-9a-f]{2})*$/

function check(ok: boolean, message: string): void {
  if (!ok) throw new InvalidDocument(message)
}

function isSafeUint(n: unknown): n is number {
  return typeof n === "number" && Number.isSafeInteger(n) && n >= 0
}

/**
 * keccak256 of the resource as two u64-length-prefixed UTF-8 strings,
 * the same framing as Mandate's canonical encoding, so `kind`/`id`
 * boundaries can't be shifted.
 */
export function resourceHash(r: Resource): Hex {
  const part = (s: string) => {
    const bytes = stringToBytes(s)
    return concat([numberToHex(bytes.length, { size: 8 }), toHex(bytes)])
  }
  return keccak256(concat([part(r.kind), part(r.id)]))
}

export function parseMandate(input: unknown): Mandate {
  const m = input as Mandate
  check(typeof m === "object" && m !== null, "mandate must be an object")
  check(HASH.test(m.id), "id must be 0x + 64 lowercase hex")
  check(ADDRESS.test(m.issuer), "issuer must be a lowercase address")
  check(ADDRESS.test(m.subject), "subject must be a lowercase address")
  check(typeof m.action === "string" && m.action.length > 0, "action must be a string")
  check(
    typeof m.resource === "object" &&
      m.resource !== null &&
      typeof m.resource.kind === "string" &&
      typeof m.resource.id === "string",
    "resource must be { kind, id }",
  )
  check(typeof m.constraints === "object" && m.constraints !== null, "constraints required")
  check(m.parent === null || HASH.test(m.parent), "parent must be null or a mandate id")
  check(typeof m.delegable === "boolean", "delegable must be a boolean")
  check(isSafeUint(m.issuedAt) && isSafeUint(m.expiresAt), "timestamps must be integers")
  check(isSafeUint(m.nonce) || typeof m.nonce === "number", "nonce must be a number")
  check(HEX.test(m.signature), "signature must be 0x hex")
  // An attribute string holds at most 128 bytes.
  check(stringToBytes(m.action).length <= 128, "action too long to index")
  check(stringToBytes(m.resource.kind).length <= 128, "resource kind too long to index")
  return m
}

export function parseRevocation(input: unknown): Revocation {
  const r = input as Revocation
  check(typeof r === "object" && r !== null, "revocation must be an object")
  check(HASH.test(r.id), "id must be 0x + 64 lowercase hex")
  check(HASH.test(r.mandateId), "mandateId must be a mandate id")
  check(ADDRESS.test(r.issuer), "issuer must be a lowercase address")
  check(isSafeUint(r.revokedAt), "revokedAt must be an integer")
  check(r.reason === null || typeof r.reason === "string", "reason must be a string or null")
  check(r.reason === null || stringToBytes(r.reason).length <= 128, "reason too long to index")
  check(HEX.test(r.signature), "signature must be 0x hex")
  return r
}

/** Queryable attributes of a mandate entity. See arkiv/schema.md. */
export function mandateAttributes(projectId: string, m: Mandate, depth: number | null) {
  return {
    project_id: str(projectId),
    entity_type: str("mandate"),
    version: str(PROTOCOL_VERSION),
    mandate_id: bytes32(m.id),
    ...(m.parent ? { parent_id: bytes32(m.parent) } : {}),
    issuer: addr(m.issuer),
    subject: addr(m.subject),
    action: str(m.action),
    resource_hash: bytes32(resourceHash(m.resource)),
    resource_kind: str(m.resource.kind),
    delegable: bool(m.delegable),
    ...(depth === null ? {} : { depth: i32(depth) }),
    expires_at: u64(BigInt(m.expiresAt)),
  }
}

/** Queryable attributes of a revocation entity. See arkiv/schema.md. */
export function revocationAttributes(projectId: string, r: Revocation) {
  return {
    project_id: str(projectId),
    entity_type: str("revocation"),
    version: str(PROTOCOL_VERSION),
    revocation_id: bytes32(r.id),
    mandate_id: bytes32(r.mandateId),
    issuer: addr(r.issuer),
    reason_code: str(r.reason ?? ""),
  }
}
