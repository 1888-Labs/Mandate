import { useEffect, useState } from "react"

import { api } from "../api"
import { explorerUrl, limits, relative, short } from "../format"
import type { Chain, Hex, MandateStatus } from "../types"

type Props = {
  entry: MandateStatus | null
  nameOf: (address: string) => string
  explorer: string
  refreshKey: number
  onCheck: (agent: Hex) => void
}

export function Inspector({ entry, nameOf, explorer, refreshKey, onCheck }: Props) {
  const [chain, setChain] = useState<Chain | null>(null)
  const id = entry?.mandate.id

  useEffect(() => {
    if (!id) return
    let live = true
    setChain(null)
    api
      .chain(id)
      .then((c) => live && setChain(c))
      .catch(() => live && setChain(null))
    return () => {
      live = false
    }
  }, [id, refreshKey])

  if (!entry) {
    return (
      <section className="card">
        <h2>Inspector</h2>
        <p className="hint">Click a mandate in the graph to see its evidence on Arkiv.</p>
      </section>
    )
  }

  const m = entry.mandate
  const creator = entry.arkiv?.creator
  const byIssuer = creator != null && creator.toLowerCase() === m.issuer.toLowerCase()
  const entity = explorerUrl(explorer, "entity", entry.arkiv?.entityKey)
  const ownRevocation = chain?.revocations.find((r) => r.revocation.mandateId === m.id)
  const ancestorRevocation = chain?.revocations.find((r) => r.revocation.mandateId !== m.id)
  const ok = entry.status === "ACTIVE"

  return (
    <section className="card inspector">
      <div className="card-head">
        <h2>Inspector</h2>
        <span className={`status ${ok ? "active" : "revoked"}`}>{entry.status}</span>
      </div>
      <p className="lead">
        <b>{nameOf(m.issuer)}</b> lets <b>{nameOf(m.subject)}</b> {m.action} {limits(m.constraints)}
        {m.delegable ? ", and pass some of that on." : "."}
      </p>
      <dl className="facts">
        <dt>Resource</dt>
        <dd className="mono">{`${m.resource.kind}:${m.resource.id}`}</dd>
        <dt>Expires</dt>
        <dd>
          {new Date(m.expiresAt * 1000).toLocaleString()} <span className="muted">({relative(m.expiresAt)})</span>
        </dd>
        <dt>Mandate ID</dt>
        <dd className="mono" title={m.id}>{short(m.id)}</dd>
        <dt>Parent</dt>
        <dd className="mono">{m.parent ? short(m.parent) : "none (root grant)"}</dd>
      </dl>

      <h3>Evidence on Arkiv</h3>
      <ul className="evidence">
        <li className={byIssuer ? "ok" : "bad"}>
          <span className="tick">{byIssuer ? "✓" : "✗"}</span>
          <span>
            Created by {creator ? nameOf(creator) : "unknown"} <span className="muted">($creator)</span>
            {byIssuer ? ", the issuer" : ", not the issuer"}
          </span>
          {entity && (
            <a href={entity} target="_blank" rel="noopener">
              entity ↗
            </a>
          )}
        </li>
        <li className={ok || ownRevocation || ancestorRevocation ? "ok" : "bad"}>
          <span className="tick">{ok ? "✓" : "•"}</span>
          <span>{ok ? "Signature, chain and expiry verified just now" : `Status: ${entry.status}`}</span>
        </li>
        {[ownRevocation, ancestorRevocation].map((r) => {
          if (!r) return null
          const link = explorerUrl(explorer, "entity", r.arkiv?.entityKey)
          return (
            <li key={r.revocation.id} className="bad">
              <span className="tick">✂</span>
              <span>
                {r.revocation.mandateId === m.id ? "Revoked" : "Ancestor revoked"} by {nameOf(r.revocation.issuer)}
                {r.revocation.reason ? ` (${r.revocation.reason})` : ""}
                <span className="muted"> · undeletable</span>
              </span>
              {link && (
                <a href={link} target="_blank" rel="noopener">
                  entity ↗
                </a>
              )}
            </li>
          )
        })}
      </ul>
      <button className="ghost small" onClick={() => onCheck(m.subject)}>
        Check {nameOf(m.subject)} →
      </button>
    </section>
  )
}
