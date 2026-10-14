import { useState } from "react"

import { api } from "../api"
import { explorerUrl, money } from "../format"
import type { Decision, Hex, Principal, Resource } from "../types"
import { Verdict } from "./Verdict"

type Props = {
  resource: Resource
  maxAmount: number
  lifetimeSeconds: number
  explorer: string
  nameOf: (address: string) => string
  /** A new name for an agent this step created. */
  onAgent: (agent: Principal) => void
  /** The data on Arkiv changed: reload the tree. */
  onChanged: () => void
  /** A decision was made, so the tree can highlight its path. */
  onDecision: (decision: Decision) => void
}

type Phase = "start" | "created" | "allowed" | "revoked" | "denied"
type Agent = { address: Hex; mandateId: Hex }

const CHECK_AMOUNT = 10

/**
 * Step 3: the revocation story, one action at a time. A fresh agent gets a
 * mandate, passes a check, loses the mandate, and fails the same check.
 */
export function RevokeLive({ resource, maxAmount, lifetimeSeconds, explorer, nameOf, onAgent, onChanged, onDecision }: Props) {
  const [phase, setPhase] = useState<Phase>("start")
  const [agent, setAgent] = useState<Agent | null>(null)
  const [working, setWorking] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [decision, setDecision] = useState<Decision | null>(null)
  const [revocationTx, setRevocationTx] = useState<string | null>(null)

  const run = async (label: string, action: () => Promise<void>) => {
    setWorking(label)
    setError(null)
    try {
      await action()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setWorking(null)
    }
  }

  const create = () =>
    run("FinanceAgent is signing a mandate and publishing it to Arkiv…", async () => {
      const r = await api.createSandbox()
      onAgent(r.agent)
      setAgent({ address: r.agent.address, mandateId: r.mandate.id })
      setDecision(null)
      setRevocationTx(null)
      setPhase("created")
      onChanged()
    })

  const check = (next: "allowed" | "denied") =>
    run("Asking Mandate…", async () => {
      if (!agent) return
      const d = await api.authorize({ subject: agent.address, action: "transfer", resource, amount: CHECK_AMOUNT, recipient: "AWS" })
      setDecision(d)
      onDecision(d)
      setPhase(next)
    })

  const revoke = () =>
    run("FinanceAgent is signing a revocation and publishing it to Arkiv…", async () => {
      if (!agent) return
      const r = (await api.revokeSandbox(agent.mandateId)) as { published?: { txHash?: string | null } }
      setRevocationTx(r.published?.txHash ?? null)
      setDecision(null)
      setPhase("revoked")
      onChanged()
    })

  // Before the agent exists it has no name yet.
  const name = agent ? nameOf(agent.address) : null
  const hours = Math.round(lifetimeSeconds / 3600)
  const steps: { label: string; done: boolean; action?: () => void; button: string }[] = [
    {
      label: `FinanceAgent grants ${name ?? "a new agent"} up to ${money(maxAmount)} → AWS for ${hours} ${hours === 1 ? "hour" : "hours"}`,
      done: phase !== "start",
      action: phase === "start" ? create : undefined,
      button: "Create agent",
    },
    {
      label: `${name ?? "The agent"} pays ${money(CHECK_AMOUNT)} to AWS`,
      done: ["allowed", "revoked", "denied"].includes(phase),
      action: phase === "created" ? () => check("allowed") : undefined,
      button: `Try ${money(CHECK_AMOUNT)}`,
    },
    {
      label: `FinanceAgent revokes ${name ? `${name}'s` : "its"} mandate`,
      done: ["revoked", "denied"].includes(phase),
      action: phase === "allowed" ? revoke : undefined,
      button: "Revoke",
    },
    {
      label: `${name ?? "The agent"} tries the same payment again`,
      done: phase === "denied",
      action: phase === "revoked" ? () => check("denied") : undefined,
      button: "Try again",
    },
  ]
  const tx = explorerUrl(explorer, "tx", revocationTx)

  return (
    <>
      <ol className="steps">
        {steps.map((s, i) => (
          <li key={i} className={s.done ? "done" : s.action ? "current" : ""}>
            <span className="num">{s.done ? "✓" : i + 1}</span>
            <span className="label">
              {s.label}
              {i === 2 && s.done && tx && (
                <a href={tx} target="_blank" rel="noopener">
                  {" "}
                  tx ↗
                </a>
              )}
            </span>
            {s.action && (
              <button className={i === 2 ? "danger" : "primary"} onClick={s.action} disabled={working !== null}>
                {s.button}
              </button>
            )}
          </li>
        ))}
      </ol>
      {working && <p className="muted working">{working}</p>}
      {error && <p className="error">{error}</p>}
      {decision && agent && <Verdict decision={decision} agent={nameOf(agent.address)} nameOf={nameOf} />}
      {phase === "denied" && (
        <p className="muted">
          The second check found the revocation on Arkiv, where anyone can verify it and nobody can delete it.{" "}
          <button className="link" onClick={() => setPhase("start")}>
            Start over
          </button>
        </p>
      )}
    </>
  )
}
