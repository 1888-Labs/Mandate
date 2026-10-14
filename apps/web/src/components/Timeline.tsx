import { explorerUrl, short } from "../format"
import type { LiveEvent } from "../types"

type Props = {
  events: (LiveEvent & { receivedAt: Date })[]
  explorer: string
  nameOf: (address: string) => string
}

const time = (d: Date) => d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hourCycle: "h23" })

export function Timeline({ events, explorer, nameOf }: Props) {
  return (
    <section className="card" id="activity">
      <div className="card-head">
        <h2>Activity</h2>
        <span className="muted small">live from Arkiv · WebSocket</span>
      </div>
      {events.length === 0 ? (
        <p className="hint">Nothing yet. Create a sandbox agent and its mandate will appear here the moment it lands on Arkiv.</p>
      ) : (
        <ol className="timeline">
          {events.map((e) => {
            const revocation = e.kind === "revocation"
            const tx = explorerUrl(explorer, "tx", e.txHash)
            const entity = explorerUrl(explorer, "entity", e.entityKey)
            return (
              <li key={`${e.txHash}-${e.entityKey}`} className={revocation ? "revocation" : "mandate"}>
                <span className="marker" aria-hidden="true">{revocation ? "✂" : "◆"}</span>
                <div className="body">
                  <div className="title">
                    {nameOf(e.creator)} {e.action === "deleted" ? "deleted an entity" : revocation ? "revoked a mandate" : "published a mandate"}
                  </div>
                  <div className="sub mono">
                    {time(e.receivedAt)} · block {e.blockNumber} · tx {short(e.txHash)}
                  </div>
                </div>
                <div className="links">
                  {tx && <a href={tx} target="_blank" rel="noopener">tx ↗</a>}
                  {entity && <a href={entity} target="_blank" rel="noopener">entity ↗</a>}
                </div>
              </li>
            )
          })}
        </ol>
      )}
    </section>
  )
}
