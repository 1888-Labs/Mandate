import { describe, expect, it } from "vitest"

import type { Hex, MandateStatus } from "../types"
import { layout, pairKey, pathPairs } from "./layout"

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

describe("layout", () => {
  it("places principals by depth with parents centred on children", () => {
    const placed = layout([edge(a(1), a(2)), edge(a(2), a(3)), edge(a(2), a(4))])
    const at = (n: number) => placed.find((p) => p.address === a(n))!
    expect(at(1).depth).toBe(0)
    expect(at(2).depth).toBe(1)
    expect(at(3).depth).toBe(2)
    expect(at(2).x).toBe((at(3).x + at(4).x) / 2)
    expect(at(1).y).toBeLessThan(at(2).y)
  })

  it("survives cycles in hostile data", () => {
    const placed = layout([edge(a(1), a(2)), edge(a(2), a(3)), edge(a(3), a(2))])
    expect(placed).toHaveLength(3)
  })

  it("turns a delegation path into edge keys", () => {
    const pairs = pathPairs([a(1), a(2), a(3)])
    expect(pairs.has(pairKey(a(1), a(2)))).toBe(true)
    expect(pairs.has(pairKey(a(2), a(3)))).toBe(true)
    expect(pairs.has(pairKey(a(1), a(3)))).toBe(false)
  })
})
