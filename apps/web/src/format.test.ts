import { describe, expect, it } from "vitest"

import { displayName, explorerUrl, limits, relative, short } from "./format"

describe("format", () => {
  it("uses the names the docs use", () => {
    expect(displayName("finance-agent")).toBe("FinanceAgent")
    expect(displayName("alice")).toBe("Alice")
    expect(displayName("sandbox-ab12")).toBe("Sandbox ab12")
  })

  it("describes limits", () => {
    expect(limits({ maxAmount: 300, allowedRecipients: ["AWS"] })).toBe("≤ $300 → AWS")
    expect(limits({ maxAmount: null, allowedRecipients: null })).toBe("any amount → anyone")
    expect(limits({ maxAmount: 1000, allowedRecipients: [] })).toBe("≤ $1,000 → no one")
  })

  it("describes expiry relative to now", () => {
    const now = 1_800_000_000_000
    expect(relative(1_800_000_000 + 2 * 86_400, now)).toBe("expires in 2d")
    expect(relative(1_800_000_000 - 3 * 3_600, now)).toBe("expired 3h ago")
  })

  it("only links well-formed values", () => {
    const base = "https://explorer"
    const key = `0x${"ab".repeat(32)}`
    expect(explorerUrl(base, "entity", key)).toBe(`${base}/entity/${key}`)
    expect(explorerUrl(base, "entity", "javascript:alert(1)")).toBeNull()
    expect(explorerUrl(base, "address", `0x${"cd".repeat(20)}`)).toBe(`${base}/address/0x${"cd".repeat(20)}`)
    expect(explorerUrl(base, "tx", null)).toBeNull()
    expect(short(key)).toBe("0xabab…abab")
  })
})
