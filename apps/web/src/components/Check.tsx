import type { FormEvent } from "react"

import type { Decision, Hex, Principal } from "../types"
import { Verdict } from "./Verdict"

export type CheckForm = { agent: Hex | ""; amount: string; recipient: string }
export type Preset = { label: string; agent: Hex; amount: number; recipient: string }

type Props = {
  agents: Principal[]
  form: CheckForm
  onChange: (form: CheckForm) => void
  onCheck: (form: CheckForm) => void
  presets: Preset[]
  checking: boolean
  result: { decision: Decision; agent: Hex } | null
  error: string | null
  nameOf: (address: string) => string
}

/** Step 2: ask Mandate whether an agent may make a payment, as a protected tool would. */
export function Check({ agents, form, onChange, onCheck, presets, checking, result, error, nameOf }: Props) {
  const submit = (e: FormEvent) => {
    e.preventDefault()
    onCheck(form)
  }
  return (
    <>
      <form className="sentence" onSubmit={submit}>
        <select aria-label="Agent" value={form.agent} onChange={(e) => onChange({ ...form, agent: e.target.value as Hex })}>
          {agents.map((a) => (
            <option key={a.address} value={a.address}>
              {nameOf(a.address)}
            </option>
          ))}
        </select>
        <span>pays $</span>
        <input
          aria-label="Amount in dollars"
          className="amount"
          type="number"
          min={0}
          step={1}
          required
          value={form.amount}
          onChange={(e) => onChange({ ...form, amount: e.target.value })}
        />
        <span>to</span>
        <input
          aria-label="Recipient"
          className="recipient"
          type="text"
          maxLength={64}
          required
          value={form.recipient}
          onChange={(e) => onChange({ ...form, recipient: e.target.value })}
        />
        <button type="submit" className="primary" disabled={checking || !form.agent}>
          {checking ? "Checking…" : "Check"}
        </button>
      </form>
      {presets.length > 0 && (
        <p className="presets">
          <span className="muted">Try</span>
          {presets.map((p) => (
            <button
              key={p.label}
              className="link"
              disabled={checking}
              onClick={() => {
                const f = { agent: p.agent, amount: String(p.amount), recipient: p.recipient }
                onChange(f)
                onCheck(f)
              }}
            >
              {p.label}
            </button>
          ))}
        </p>
      )}
      {error && <p className="error">{error}</p>}
      {result && !error && <Verdict decision={result.decision} agent={nameOf(result.agent)} nameOf={nameOf} />}
    </>
  )
}
