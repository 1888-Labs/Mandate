import { explorerUrl } from "../format"
import type { LiveStatus } from "../hooks/useLiveEvents"
import type { LiveEvent } from "../types"

type Props = {
  status: LiveStatus
  events: (LiveEvent & { receivedAt: Date })[]
  explorer: string
  nameOf: (address: string) => string
}

const time = (d: Date) => d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hourCycle: "h23" })

function describe(e: LiveEvent, who: string): string {
  if (e.action === "deleted") return `${who}'s entity left Arkiv`
  return e.kind === "revocation" ? `${who} revoked a mandate` : `${who} published a mandate`
}

/** Mandate entities landing on Arkiv, pushed to the page as they happen. */
export function LiveFeed({ status, events, explorer, nameOf }: Props) {
  if (status === "off") return <p className="muted">Live events are off: this server isn't connected to Arkiv.</p>
  if (events.length === 0) return <p className="muted">Waiting for the next mandate or revocation to land on Arkiv. Step 3 makes one.</p>
  return (
    <ol className="feed">
      {events.map((e) => {
        const tx = explorerUrl(explorer, "tx", e.txHash)
        return (
          <li key={`${e.txHash}-${e.entityKey}`} className={e.kind}>
            <span className="time">{time(e.receivedAt)}</span>
            <span>{describe(e, nameOf(e.creator))}</span>
            {tx && (
              <a href={tx} target="_blank" rel="noopener">
                tx ↗
              </a>
            )}
          </li>
        )
      })}
    </ol>
  )
}
