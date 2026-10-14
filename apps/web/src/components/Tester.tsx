import type { FormEvent } from "react"

import { limits, money, short } from "../format"
import type { Decision, Hex, Principal } from "../types"

export type TesterForm = { agent: Hex | ""; amount: string; recipient: string }

type Props = {
  agents: Principal[]
  resourceLabel: string
  form: TesterForm
  onChange: (form: TesterForm) => void
  onCheck: () => void
  onPreset: (preset: Preset) => void
  checking: boolean
  decision: Decision | null
  error: string | null
  nameOf: (address: string) => string
  presets: Preset[]
}

export type Preset = { label: string; agent: Hex; amount: number; recipient: string }

export function Tester(props: Props) {
  const { agents, resourceLabel, form, onChange, onCheck, onPreset, checking, decision, error, nameOf, presets } = props
  const submit = (e: FormEvent) => {
    e.preventDefault()
    onCheck()
  }
  return (
    <section className="card" id="tester">
      <h2>Check an action</h2>
      <p className="hint">
        Ask Mandate whether an agent may make a transfer from <code>{resourceLabel}</code>.
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
            Amount ($)
            <input
              type="number"
              min={0}
              step={1}
              required
              value={form.amount}
              onChange={(e) => onChange({ ...form, amount: e.target.value })}
            />
          </label>
          <label>
            Recipient
            <input
              type="text"
              maxLength={64}
              required
              value={form.recipient}
              onChange={(e) => onChange({ ...form, recipient: e.target.value })}
            />
          </label>
        </div>
        <button type="submit" className="primary" disabled={checking || !form.agent}>
          {checking ? "Checking…" : "Check"}
        </button>
      </form>
      {error && <div className="result deny"><div className="error">{error}</div></div>}
      {decision && !error && <DecisionView decision={decision} nameOf={nameOf} />}
      <div className="presets">
        <span className="muted">Try:</span>
        {presets.map((p) => (
          <button key={p.label} className="ghost small" onClick={() => onPreset(p)} disabled={checking}>
            {p.label}
          </button>
        ))}
      </div>
    </section>
  )
}

function DecisionView({ decision: d, nameOf }: { decision: Decision; nameOf: (a: string) => string }) {
  const allow = d.decision === "ALLOW"
  const details: string[] = []
  if (!allow) {
    if (d.requestedAmount !== undefined && d.maximumAmount !== undefined) {
      details.push(`requested ${d.requestedAmount === null ? "nothing" : money(d.requestedAmount)}, maximum ${money(d.maximumAmount)}`)
    }
    if (d.allowedRecipients) details.push(`allowed recipients: ${d.allowedRecipients.join(", ") || "none"}`)
    if (d.revokedMandateId) details.push(`revoked mandate ${short(d.revokedMandateId)}`)
  }
  return (
    <div className={`result ${allow ? "allow" : "deny"}`}>
      <div className="verdict">
        {allow ? "✓ ALLOW" : "✗ DENY"}
        {!allow && <span className="code">{d.reason}</span>}
      </div>
      <p>{allow ? `Within ${limits(d.effectiveConstraints)}, granted by ${nameOf(d.rootPrincipal)}.` : d.message}</p>
      {details.length > 0 && <p className="muted">{details.join(" · ")}</p>}
      {d.delegationPath.length > 1 && (
        <div className="path">
          <span className="muted">Chain:</span>
          {d.delegationPath.map((a, i) => (
            <span key={`${a}-${i}`} className="path-step">
              {i > 0 && <span className="arrow">→</span>}
              <span className="pill">{nameOf(a)}</span>
            </span>
          ))}
        </div>
      )}
    </div>
  )
}
