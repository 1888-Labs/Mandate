import { useEffect, useMemo, useState } from "react"

import { api } from "../api"
import { explorerUrl, isBurn, limits, relative, short } from "../format"
import { pairKey, pathPairs, tree } from "../graph/tree"
import type { Chain, Hex, MandateStatus } from "../types"

type Props = {
  edges: MandateStatus[]
  loading: boolean
  error: string | null
  nameOf: (address: string) => string
  explorer: string
  /** Delegation path of the last decision, highlighted in the tree. */
  path: Hex[]
  /** The revoked mandate that cut the last decision, if any. */
  cut: Hex | null
  selected: Hex | null
  onSelect: (id: Hex | null) => void
  /** Bumped whenever the data on Arkiv changed, to re-read the open mandate. */
  refreshKey: number
}

/** Step 1: every mandate, as a tree of who delegated what to whom. */
export function Delegations(props: Props) {
  const { edges, loading, error, nameOf, explorer, path, cut, selected, onSelect, refreshKey } = props
  const rows = useMemo(() => tree(edges), [edges])
  const onPath = useMemo(() => pathPairs(path), [path])

  if (error) return <p className="error">{error}</p>
  if (rows.length === 0) return <p className="muted">{loading ? "Reading mandates from Arkiv…" : "No active mandates on Arkiv yet."}</p>

  return (
    <ul className="tree">
      {rows.map(({ entry, depth }) => {
        const m = entry.mandate
        const open = m.id === selected
        const active = entry.status === "ACTIVE"
        const cls = [
          "row",
          open && "open",
          !active && "inactive",
          onPath.has(pairKey(m.issuer, m.subject)) && "on-path",
          m.id === cut && "cut",
        ]
          .filter(Boolean)
          .join(" ")
        return (
          <li key={m.id} style={{ "--depth": depth } as React.CSSProperties}>
            <button className={cls} onClick={() => onSelect(open ? null : m.id)} aria-expanded={open}>
              <span className="who">
                <span className="muted">{nameOf(m.issuer)} →</span> <b>{nameOf(m.subject)}</b>
              </span>
              <span className="limits">{limits(m.constraints)}</span>
              <span className={`pill ${active ? "ok" : "bad"}`}>{active ? "Active" : statusLabel(entry.status)}</span>
            </button>
            {open && <Details entry={entry} nameOf={nameOf} explorer={explorer} refreshKey={refreshKey} />}
          </li>
        )
      })}
    </ul>
  )
}

function statusLabel(status: string): string {
  if (status === "REVOKED") return "Revoked"
  if (status === "ANCESTOR_REVOKED") return "Cut off"
  if (status === "EXPIRED") return "Expired"
  return status
}

/** What a mandate grants, and the evidence for it on Arkiv. */
function Details({ entry, nameOf, explorer, refreshKey }: { entry: MandateStatus; nameOf: (a: string) => string; explorer: string; refreshKey: number }) {
  const m = entry.mandate
  const [chain, setChain] = useState<Chain | null>(null)

  useEffect(() => {
    let live = true
    api
      .chain(m.id)
      .then((c) => live && setChain(c))
      .catch(() => live && setChain(null))
    return () => {
      live = false
    }
  }, [m.id, refreshKey])

  const creator = entry.arkiv?.creator
  const owner = entry.arkiv?.owner
  const byIssuer = creator != null && creator.toLowerCase() === m.issuer.toLowerCase()
  const entity = explorerUrl(explorer, "entity", entry.arkiv?.entityKey)

  return (
    <div className="details">
      <p>
        {nameOf(m.issuer)} lets {nameOf(m.subject)} {m.action} {limits(m.constraints)} from{" "}
        <code>{`${m.resource.kind}:${m.resource.id}`}</code>
        {m.delegable ? ", and pass some of that on." : "."}{" "}
        <span className="muted">{relative(m.expiresAt)}.</span>
      </p>
      <ul className="evidence">
        <li className={byIssuer ? "ok" : "bad"}>
          <span className="mark">{byIssuer ? "✓" : "✗"}</span>
          <span>
            Published by {creator ? nameOf(creator) : "an unknown wallet"}
            {byIssuer ? ", the issuer" : ", not the issuer, so it's ignored"}
            <span className="muted"> · Arkiv's $creator</span>
          </span>
          {entity && <a href={entity} target="_blank" rel="noopener">entity ↗</a>}
        </li>
        {entry.arkiv?.entityKey && (
          <li>
            <span className="mark">·</span>
            <span>
              Read-only{owner ? `, owned by ${nameOf(owner)}` : ""}. Leaves Arkiv when the mandate expires,{" "}
              {new Date(m.expiresAt * 1000).toLocaleString()}.
            </span>
          </li>
        )}
        {chain?.revocations.map(({ revocation: r, arkiv }) => {
          const link = explorerUrl(explorer, "entity", arkiv?.entityKey)
          return (
            <li key={r.id} className="bad">
              <span className="mark">✕</span>
              <span>
                {r.mandateId === m.id ? "Revoked" : `A mandate above it (${short(r.mandateId)}) was revoked`} by {nameOf(r.issuer)}
                {r.reason ? ` (${r.reason})` : ""}.
                {isBurn(arkiv?.owner) && <span className="muted"> Owned by the burn address: nobody can delete it.</span>}
              </span>
              {link && <a href={link} target="_blank" rel="noopener">entity ↗</a>}
            </li>
          )
        })}
      </ul>
    </div>
  )
}
