import type { Hex, MandateStatus } from "../types"

export const NODE_WIDTH = 200
export const NODE_HEIGHT = 56
/** Horizontal distance between siblings, and vertical distance between generations. */
const COLUMN = 250
const ROW = 170

export type Placed = { address: Hex; depth: number; x: number; y: number }

/**
 * A top-down tree: each generation of delegation one row lower, leaves side by
 * side in order, and every parent centred above its children.
 */
export function layout(edges: MandateStatus[]): Placed[] {
  const children = new Map<string, Hex[]>()
  const subjects = new Set<string>()
  for (const e of edges) {
    const list = children.get(e.mandate.issuer) ?? []
    if (!list.includes(e.mandate.subject)) list.push(e.mandate.subject)
    children.set(e.mandate.issuer, list)
    subjects.add(e.mandate.subject)
  }
  const roots = [...new Set(edges.map((e) => e.mandate.issuer))].filter((i) => !subjects.has(i))

  const placed = new Map<string, Placed>()
  let nextRow = 0
  const visit = (address: Hex, depth: number): number => {
    const existing = placed.get(address)
    if (existing) return existing.x
    // Reserve the node first, so a cycle in hostile data can't loop forever.
    placed.set(address, { address, depth, x: 0, y: depth * ROW })
    const kids = (children.get(address) ?? []).filter((k) => !placed.has(k))
    const xs = kids.map((k) => visit(k, depth + 1))
    const x = xs.length ? (Math.min(...xs) + Math.max(...xs)) / 2 : nextRow++ * COLUMN
    placed.set(address, { address, depth, x, y: depth * ROW })
    return x
  }
  for (const r of roots) visit(r, 0)
  return [...placed.values()]
}

/** Consecutive pairs of a delegation path, as `issuer>subject` keys. */
export function pathPairs(path: Hex[]): Set<string> {
  const pairs = new Set<string>()
  for (let i = 1; i < path.length; i++) pairs.add(`${path[i - 1]?.toLowerCase()}>${path[i]?.toLowerCase()}`)
  return pairs
}

export const pairKey = (issuer: string, subject: string) => `${issuer.toLowerCase()}>${subject.toLowerCase()}`
