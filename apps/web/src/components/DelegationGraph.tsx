import { colorFor, explorerUrl, limits, relative, short } from "../format"
import type { Hex, MandateStatus } from "../types"

type Props = {
  edges: MandateStatus[]
  nameOf: (address: string) => string
  explorer: string
  loading: boolean
  error: string | null
  onRefresh: () => void
  onSelect: (agent: Hex) => void
}

function statusClass(status: string): string {
  if (status === "ACTIVE") return "active"
  if (status === "REVOKED" || status === "ANCESTOR_REVOKED") return "revoked"
  if (status === "EXPIRED") return "expired"
  return "denied"
}

export function DelegationGraph({ edges, nameOf, explorer, loading, error, onRefresh, onSelect }: Props) {
  const subjects = new Set(edges.map((e) => e.mandate.subject))
  const roots = [...new Set(edges.map((e) => e.mandate.issuer))].filter((i) => !subjects.has(i))

  const node = (address: Hex, depth: number) => {
    const name = nameOf(address)
    const children = edges
      .filter((e) => e.mandate.issuer === address)
      .sort((a, b) => a.mandate.issuedAt - b.mandate.issuedAt)
    return (
      <div className="tree" key={`${address}-${depth}`}>
        <div className="node">
          <span className="avatar" style={{ background: colorFor(name) }}>
            {name.slice(0, 1).toUpperCase()}
          </span>
          <div className="who">
            <b>{name}</b>
            {depth === 0 ? (
              <span className="small muted">trust anchor · owns the treasury wallet</span>
            ) : (
              <span className="mono small">{short(address)}</span>
            )}
          </div>
        </div>
        {children.length > 0 && (
          <div className="children">
            {children.map((e) => (
              <div className="branch" key={e.mandate.id}>
                <Edge entry={e} nameOf={nameOf} explorer={explorer} onSelect={onSelect} />
                {depth < 8 && node(e.mandate.subject, depth + 1)}
              </div>
            ))}
          </div>
        )}
      </div>
    )
  }

  return (
    <section className="card graph-card">
      <div className="card-head">
        <h2>Delegation graph</h2>
        <button className="ghost" onClick={onRefresh} title="Reload from Arkiv" disabled={loading}>
          ↻
        </button>
      </div>
      <p className="hint">Who granted what. Each link is a signed mandate; authority only narrows going down.</p>
      <div className="graph">
        {error ? (
          <p className="error">{error}</p>
        ) : edges.length === 0 ? (
          <p className="muted">{loading ? "Loading from Arkiv…" : "No active mandates found on Arkiv."}</p>
        ) : (
          roots.map((r) => node(r, 0))
        )}
      </div>
    </section>
  )
}

function Edge({
  entry,
  nameOf,
  explorer,
  onSelect,
}: {
  entry: MandateStatus
  nameOf: (a: string) => string
  explorer: string
  onSelect: (agent: Hex) => void
}) {
  const m = entry.mandate
  const cls = statusClass(entry.status)
  const creator = entry.arkiv?.creator
  const byIssuer = creator != null && creator.toLowerCase() === m.issuer.toLowerCase()
  const entity = explorerUrl(explorer, "entity", entry.arkiv?.entityKey)
  return (
    <div className={`edge ${cls}`} title="Check this agent" onClick={() => onSelect(m.subject)}>
      <div className="edge-top">
        <span className="limits">
          {m.action} {limits(m.constraints)}
        </span>
        <span className={`status ${cls}`}>{entry.status}</span>
      </div>
      <div className="meta">
        <span>
          {nameOf(m.issuer)} → {nameOf(m.subject)}
        </span>
        {m.delegable && <span>may delegate</span>}
        <span>{relative(m.expiresAt)}</span>
        {creator && (
          <span className={byIssuer ? "verified" : "unverified"}>
            {byIssuer ? `✓ published by ${nameOf(creator)} ($creator)` : `✗ creator ${short(creator)} is not the issuer`}
          </span>
        )}
        {entity && (
          <a href={entity} target="_blank" rel="noopener" onClick={(e) => e.stopPropagation()}>
            Arkiv entity ↗
          </a>
        )}
      </div>
    </div>
  )
}
