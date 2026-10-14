import type { FormEvent } from "react"

import { explain } from "../explain"
import { short } from "../format"
import type { Decision, Hex, Principal } from "../types"

export type TesterForm = { agent: Hex | ""; amount: string; recipient: string }
export type Preset = { label: string; agent: Hex; amount: number; recipient: string }

type Props = {
  agents: Principal[]
  resourceLabel: string
  form: TesterForm
  onChange: (form: TesterForm) => void
  onCheck: () => void
  onPreset: (preset: Preset) => void
  checking: boolean
  decision: { result: Decision; agent: Hex } | null
  error: string | null
  nameOf: (address: string) => string
  presets: Preset[]
  onInspect: (mandateId: Hex) => void
}

export function Tester(props: Props) {
  const { agents, resourceLabel, form, onChange, onCheck, onPreset, checking, decision, error, nameOf, presets, onInspect } = props
  const submit = (e: FormEvent) => {
    e.preventDefault()
    onCheck()
  }
  return (
    <section className="card console" id="check">
      <h2>Check an action</h2>
      <p className="hint">
        What a protected tool asks Mandate before acting on <code>{resourceLabel}</code>.
      </p>
      <form className="form" onSubmit={submit}>
        <label>
          Agent
          <select value={form.agent} onChange={(e) => onChange({ ...form, agent: e.target.value as Hex })}>
            {agents.map((a) => (
              <option key={a.address} value={a.address}>
                {nameOf(a.address)}
              </option>
            ))}
          </select>
        </label>
        <div className="row">
          <label>
            Transfer ($)
            <input type="number" min={0} step={1} required value={form.amount} onChange={(e) => onChange({ ...form, amount: e.target.value })} />
          </label>
          <label>
            To
            <input type="text" maxLength={64} required value={form.recipient} onChange={(e) => onChange({ ...form, recipient: e.target.value })} />
          </label>
        </div>
        <button type="submit" className="primary" disabled={checking || !form.agent}>
          {checking ? "Asking Mandate…" : "Check"}
        </button>
      </form>
      <div className="presets">
        {presets.map((p) => (
          <button key={p.label} className="chip-button" onClick={() => onPreset(p)} disabled={checking}>
            {p.label}
          </button>
        ))}
      </div>
      {error && (
        <div className="verdict-card deny">
          <p className="error">{error}</p>
        </div>
      )}
      {decision && !error && (
        <DecisionView decision={decision.result} agent={nameOf(decision.agent)} nameOf={nameOf} onInspect={onInspect} />
      )}
    </section>
  )
}

function DecisionView({
  decision: d,
  agent,
  nameOf,
  onInspect,
}: {
  decision: Decision
  agent: string
  nameOf: (a: string) => string
  onInspect: (id: Hex) => void
}) {
  const allow = d.decision === "ALLOW"
  return (
    <div key={`${d.decision}-${d.mandateId}-${explain(d, agent, nameOf)}`} className={`verdict-card ${allow ? "allow" : "deny"} enter`} aria-live="polite">
      <div className="verdict-head">
        <span className="verdict-icon" aria-hidden="true">{allow ? "✓" : "✕"}</span>
        <span className="verdict">{allow ? "ALLOW" : "DENY"}</span>
        {!allow && <span className="code">{d.reason}</span>}
      </div>
      <p className="explanation">{explain(d, agent, nameOf)}</p>
      {d.delegationPath.length > 1 && (
        <div className="path">
          {d.delegationPath.map((a, i) => (
            <span key={`${a}-${i}`} className="path-step">
              {i > 0 && <span className="arrow">→</span>}
              <span className="pill">{nameOf(a)}</span>
            </span>
          ))}
        </div>
      )}
      <div className="evidence-links">
        {d.mandateId && (
          <button className="link" onClick={() => onInspect(d.mandateId!)}>
            decided by mandate {short(d.mandateId)} →
          </button>
        )}
        {!allow && d.revokedMandateId && (
          <button className="link" onClick={() => onInspect(d.revokedMandateId!)}>
            revoked: {short(d.revokedMandateId)} →
          </button>
        )}
      </div>
    </div>
  )
}
