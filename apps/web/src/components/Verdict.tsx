import { explain } from "../explain"
import type { Decision } from "../types"

type Props = {
  decision: Decision
  /** Display name of the agent that asked. */
  agent: string
  nameOf: (address: string) => string
}

/** Allowed or denied, why in one sentence, and the chain of authority behind it. */
export function Verdict({ decision: d, agent, nameOf }: Props) {
  const allow = d.decision === "ALLOW"
  return (
    <div className={`verdict ${allow ? "allow" : "deny"}`} aria-live="polite">
      <div className="verdict-head">
        <b>{allow ? "ALLOW" : "DENY"}</b>
        {!allow && <code className="code">{d.reason}</code>}
      </div>
      <p>{explain(d, agent, nameOf)}</p>
      {d.delegationPath.length > 1 && (
        <p className="path">
          {d.delegationPath.map((a, i) => (
            <span key={`${a}-${i}`}>
              {i > 0 && <span className="arrow"> → </span>}
              {nameOf(a)}
            </span>
          ))}
        </p>
      )}
    </div>
  )
}
