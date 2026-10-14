import { describe, expect, it } from "vitest"

import { explain } from "./explain"
import type { Decision } from "./types"

const nameOf = (a: string) => (a === "0xa" ? "Alice" : a)

describe("explain", () => {
  it("explains denials in plain language", () => {
    const d: Decision = {
      decision: "DENY",
      reason: "AMOUNT_EXCEEDED",
      message: "",
      delegationPath: [],
      requestedAmount: 700,
      maximumAmount: 300,
    }
    expect(explain(d, "InvoiceAgent", nameOf)).toBe("InvoiceAgent asked for $700, but its mandate allows at most $300.")
    expect(
      explain({ ...d, reason: "RECIPIENT_NOT_ALLOWED", requestedRecipient: "Mallory", allowedRecipients: ["AWS"] }, "Bot", nameOf),
    ).toBe("Bot may only pay AWS, not Mallory.")
  })

  it("explains an allow with its root", () => {
    const d: Decision = {
      decision: "ALLOW",
      mandateId: "0x1",
      rootPrincipal: "0xa",
      delegationPath: [],
      chain: [],
      effectiveConstraints: { maxAmount: 300, allowedRecipients: ["AWS"] },
    }
    expect(explain(d, "InvoiceAgent", nameOf)).toContain("traces back to Alice")
  })
})
