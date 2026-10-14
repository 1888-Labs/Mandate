import type { Constraints } from "./types"

const HEX64 = /^0x[0-9a-fA-F]{64}$/
const ADDRESS = /^0x[0-9a-fA-F]{40}$/

/** "finance-agent" → "FinanceAgent", "sandbox-ab12" → "Sandbox ab12": the names the docs use. */
export function displayName(name: string): string {
  if (name.startsWith("sandbox-")) return `Sandbox ${name.slice("sandbox-".length)}`
  return name
    .split("-")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join("")
}

/** `0x1234…cdef` */
export function short(hex: string): string {
  return hex.length > 14 ? `${hex.slice(0, 6)}…${hex.slice(-4)}` : hex
}

export function money(n: number): string {
  return `$${n.toLocaleString("en-US")}`
}

/** `≤ $300 → AWS`, `any amount → anyone` */
export function limits(c: Constraints | null | undefined): string {
  const amount = c?.maxAmount == null ? "any amount" : `≤ ${money(c.maxAmount)}`
  const to =
    c?.allowedRecipients == null ? "anyone" : c.allowedRecipients.length ? c.allowedRecipients.join(", ") : "no one"
  return `${amount} → ${to}`
}

/** `expires in 24d`, `expired 3h ago` */
export function relative(unixSeconds: number, nowMs = Date.now()): string {
  const s = unixSeconds - nowMs / 1000
  const abs = Math.abs(s)
  const span =
    abs >= 86_400 ? `${Math.round(abs / 86_400)}d` : abs >= 3_600 ? `${Math.round(abs / 3_600)}h` : `${Math.max(1, Math.round(abs / 60))}m`
  return s >= 0 ? `expires in ${span}` : `expired ${span} ago`
}

/**
 * An explorer link, or null if the value isn't a well-formed key, hash or
 * address. Values come from Arkiv, where anyone can write.
 */
export function explorerUrl(base: string, kind: "entity" | "tx" | "address", value: string | null | undefined): string | null {
  if (!value) return null
  const ok = kind === "address" ? ADDRESS.test(value) : HEX64.test(value)
  return ok ? `${base}/${kind}/${value}` : null
}

const COLORS = ["#8b7bff", "#22c55e", "#0ea5e9", "#f59e0b", "#ec4899", "#14b8a6", "#a855f7"]

/** A stable colour per name. */
export function colorFor(name: string): string {
  let h = 0
  for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) >>> 0
  return COLORS[h % COLORS.length] ?? COLORS[0]!
}
