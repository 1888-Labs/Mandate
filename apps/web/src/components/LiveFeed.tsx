import { explorerUrl } from "../format"
import type { LiveEvent } from "../types"

type Props = {
  events: (LiveEvent & { receivedAt: Date })[]
  explorer: string
  nameOf: (address: string) => string
}

const time = (d: Date) => d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hourCycle: "h23" })

export function LiveFeed({ events, explorer, nameOf }: Props) {
  return (
    <section className="card feed-card">
      <div className="card-head">
        <h2>Live on Arkiv</h2>
        <span className="muted small">entity events over WebSocket · no polling</span>
      </div>
      <ol className="feed">
        {events.length === 0 && (
          <li className="muted empty">Waiting for activity… create a sandbox agent to see one arrive.</li>
        )}
        {events.map((e) => {
          const who = nameOf(e.creator)
          const what =
            e.action === "deleted"
              ? `${who}'s entity was deleted`
              : e.kind === "revocation"
                ? `${who} published a revocation`
                : `${who} published a mandate`
          const tx = explorerUrl(explorer, "tx", e.txHash)
          const entity = explorerUrl(explorer, "entity", e.entityKey)
          return (
            <li key={`${e.txHash}-${e.entityKey}`} className="new">
              <span className="time">{time(e.receivedAt)}</span>
              <span>
                {what}
                <span className="muted"> · block {e.blockNumber}</span>
              </span>
              <span className="links">
                {tx && <a href={tx} target="_blank" rel="noopener">tx ↗</a>}
                {entity && <a href={entity} target="_blank" rel="noopener">entity ↗</a>}
              </span>
            </li>
          )
        })}
      </ol>
    </section>
  )
}
