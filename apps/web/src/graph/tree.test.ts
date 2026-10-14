import { describe, expect, it } from "vitest"

import type { Hex, MandateStatus } from "../types"
import { pairKey, pathPairs, tree } from "./tree"

const a = (n: number) => `0x${String(n).repeat(40).slice(0, 40)}` as Hex
const edge = (issuer: Hex, subject: Hex): MandateStatus => ({
  status: "ACTIVE",
  arkiv: null,
  mandate: {
    id: `0x${"0".repeat(64)}`,
    issuer,
    subject,
    action: "transfer",
    resource: { kind: "wallet", id: "x" },
    constraints: { maxAmount: 1, allowedRecipients: null },
    parent: null,
    delegable: true,
    issuedAt: 0,
    expiresAt: 0,
    nonce: 0,
    signature: "0x",
  },
})

/** `depth:issuer>subject`, using each address's repeated digit. */
const shape = (rows: ReturnType<typeof tree>) =>
  rows.map((r) => `${r.depth}:${r.entry.mandate.issuer[2]}>${r.entry.mandate.subject[2]}`)

describe("tree", () => {
  it("lists each grant followed by what is delegated beneath it", () => {
    const rows = tree([edge(a(2), a(4)), edge(a(1), a(2)), edge(a(2), a(3))])
    expect(shape(rows)).toEqual(["0:1>2", "1:2>4", "1:2>3"])
  })

  it("survives cycles in hostile data and still lists every mandate", () => {
    expect(tree([edge(a(1), a(2)), edge(a(2), a(3)), edge(a(3), a(2))])).toHaveLength(3)
  })

  it("lists a pure cycle that has no root", () => {
    expect(tree([edge(a(1), a(2)), edge(a(2), a(1))])).toHaveLength(2)
  })

  it("turns a delegation path into edge keys", () => {
    const pairs = pathPairs([a(1), a(2), a(3)])
    expect(pairs.has(pairKey(a(1), a(2)))).toBe(true)
    expect(pairs.has(pairKey(a(2), a(3)))).toBe(true)
    expect(pairs.has(pairKey(a(1), a(3)))).toBe(false)
  })
})
