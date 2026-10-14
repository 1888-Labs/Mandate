import type { Hex, MandateStatus } from "../types"

/** One mandate in the delegation tree, `depth` levels below the root grant. */
export type Row = { entry: MandateStatus; depth: number }

/**
 * Mandates in reading order: each root grant, then everything delegated
 * beneath it, depth-first. Each agent is expanded once, so a cycle in hostile
 * data can't loop forever.
 */
export function tree(edges: MandateStatus[]): Row[] {
  const byIssuer = new Map<string, MandateStatus[]>()
  const subjects = new Set<string>()
  for (const e of edges) {
    const issuer = e.mandate.issuer.toLowerCase()
    byIssuer.set(issuer, [...(byIssuer.get(issuer) ?? []), e])
    subjects.add(e.mandate.subject.toLowerCase())
  }

  const rows: Row[] = []
  const expanded = new Set<string>()
  const visit = (issuer: string, depth: number) => {
    if (expanded.has(issuer)) return
    expanded.add(issuer)
    for (const e of byIssuer.get(issuer) ?? []) {
      rows.push({ entry: e, depth })
      visit(e.mandate.subject.toLowerCase(), depth + 1)
    }
  }
  for (const issuer of byIssuer.keys()) if (!subjects.has(issuer)) visit(issuer, 0)
  // Mandates only reachable through a cycle still get listed.
  for (const issuer of byIssuer.keys()) visit(issuer, 0)
  return rows
}

/** Consecutive pairs of a delegation path, as `issuer>subject` keys. */
export function pathPairs(path: Hex[]): Set<string> {
  const pairs = new Set<string>()
  for (let i = 1; i < path.length; i++) pairs.add(pairKey(path[i - 1]!, path[i]!))
  return pairs
}

export const pairKey = (issuer: string, subject: string) => `${issuer.toLowerCase()}>${subject.toLowerCase()}`
