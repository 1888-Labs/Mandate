import { limits, money } from "./format"
import type { Decision } from "./types"

/** A one-sentence, plain-language explanation of a decision. */
export function explain(d: Decision, agent: string, nameOf: (a: string) => string): string {
  if (d.decision === "ALLOW") {
    return `${agent} may do this: it's within ${limits(d.effectiveConstraints)}, authority that traces back to ${nameOf(d.rootPrincipal)}.`
  }
  switch (d.reason) {
    case "AMOUNT_EXCEEDED":
      return d.requestedAmount == null
        ? `${agent} didn't say how much, but its mandate caps transfers at ${money(d.maximumAmount ?? 0)}.`
        : `${agent} asked for ${money(d.requestedAmount)}, but its mandate allows at most ${money(d.maximumAmount ?? 0)}.`
    case "RECIPIENT_NOT_ALLOWED":
      return `${agent} may only pay ${(d.allowedRecipients ?? []).join(", ") || "no one"}, not ${d.requestedRecipient ?? "an unnamed recipient"}.`
    case "REVOKED":
      return `${agent}'s mandate was revoked by whoever granted it, so it no longer grants anything.`
    case "ANCESTOR_REVOKED":
      return `A mandate higher up ${agent}'s chain was revoked, which cuts off everything delegated beneath it.`
    case "EXPIRED":
      return `A mandate in ${agent}'s chain has expired.`
    case "NO_MANDATE":
      return `${agent} holds no mandate for this action on this resource.`
    case "UNTRUSTED_ROOT":
      return `${agent}'s chain doesn't start with a trusted owner of this resource, so nobody with real authority granted it.`
    case "INVALID_SIGNATURE":
      return `A mandate in ${agent}'s chain has an invalid signature: it may be forged.`
    case "INVALID_DELEGATION":
      return `A link in ${agent}'s chain tries to grant more than its parent allows.`
    default:
      return d.message
  }
}
