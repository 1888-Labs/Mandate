import { useCallback, useEffect, useMemo, useRef, useState } from "react"

import { api } from "./api"
import { REPO_URL } from "./brand"
import { Check, type CheckForm, type Preset } from "./components/Check"
import { Delegations } from "./components/Delegations"
import { Header } from "./components/Header"
import { LiveFeed } from "./components/LiveFeed"
import { RevokeLive } from "./components/RevokeLive"
import { displayName, short } from "./format"
import { useLiveEvents } from "./hooks/useLiveEvents"
import type { Decision, DemoInfo, Hex, MandateStatus, Principal } from "./types"

export function App() {
  const [demo, setDemo] = useState<DemoInfo | null>(null)
  const [fatal, setFatal] = useState<string | null>(null)
  const [names, setNames] = useState<Map<string, string>>(new Map())
  const [extraAgents, setExtraAgents] = useState<Principal[]>([])

  const [edges, setEdges] = useState<MandateStatus[]>([])
  const [loading, setLoading] = useState(true)
  const [treeError, setTreeError] = useState<string | null>(null)
  const [selected, setSelected] = useState<Hex | null>(null)
  const [refreshKey, setRefreshKey] = useState(0)

  const [form, setForm] = useState<CheckForm>({ agent: "", amount: "200", recipient: "AWS" })
  const [result, setResult] = useState<{ decision: Decision; agent: Hex } | null>(null)
  const [checking, setChecking] = useState(false)
  const [checkError, setCheckError] = useState<string | null>(null)
  /** The last decision from either step, highlighted in the tree. */
  const [lastDecision, setLastDecision] = useState<Decision | null>(null)

  const nameOf = useCallback((address: string) => names.get(address.toLowerCase()) ?? short(address), [names])

  const addName = useCallback((p: Principal) => setNames((n) => new Map(n).set(p.address.toLowerCase(), displayName(p.name))), [])

  // Every agent that can hold a mandate: the demo cast (minus Alice, the root) and sandbox agents.
  const agents: Principal[] = useMemo(() => {
    if (!demo) return []
    const all = [...demo.principals.filter((p) => p.name !== "alice"), ...demo.sandboxAgents, ...extraAgents]
    return all.filter((p, i) => all.findIndex((q) => q.address === p.address) === i)
  }, [demo, extraAgents])

  const loadTree = useCallback(async () => {
    if (agents.length === 0) return
    setLoading(true)
    try {
      const lists = await Promise.all(agents.map((a) => api.agentMandates(a.address).catch(() => [])))
      // One row per (issuer, subject): the most recently issued mandate.
      const latest = new Map<string, MandateStatus>()
      for (const entry of lists.flat()) {
        const key = `${entry.mandate.issuer}>${entry.mandate.subject}`
        const seen = latest.get(key)
        if (!seen || seen.mandate.issuedAt < entry.mandate.issuedAt) latest.set(key, entry)
      }
      setEdges([...latest.values()])
      setTreeError(null)
      setRefreshKey((k) => k + 1)
    } catch (e) {
      setTreeError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [agents])

  useEffect(() => {
    api
      .demo()
      .then((d) => {
        setDemo(d)
        setNames(new Map([...d.principals, ...d.sandboxAgents].map((p) => [p.address.toLowerCase(), displayName(p.name)])))
        const invoice = d.principals.find((p) => p.name === "invoice-agent")
        if (invoice) setForm((f) => ({ ...f, agent: invoice.address }))
      })
      .catch((e: unknown) => {
        const message = e instanceof Error ? e.message : String(e)
        // 502/504 from the dev proxy, or a network error: the API isn't running.
        const unreachable = /^50[24]\b|Failed to fetch|NetworkError/.test(message)
        setFatal(unreachable ? "Can't reach the Mandate API. Start it with scripts/run-local.sh, then reload." : message)
      })
  }, [])

  useEffect(() => {
    void loadTree()
  }, [loadTree])

  // Open InvoiceAgent's mandate first: it's the one the checks are about.
  useEffect(() => {
    if (selected || edges.length === 0) return
    const invoice = demo?.principals.find((p) => p.name === "invoice-agent")?.address
    setSelected((edges.find((e) => e.mandate.subject === invoice) ?? edges[0])?.mandate.id ?? null)
  }, [edges, demo, selected])

  // Arkiv entity events: reload the tree shortly after something lands.
  const refreshTimer = useRef<ReturnType<typeof setTimeout>>(undefined)
  const live = useLiveEvents(Boolean(demo?.liveEvents), () => {
    clearTimeout(refreshTimer.current)
    refreshTimer.current = setTimeout(() => void loadTree(), 400)
  })

  const runCheck = useCallback(
    async (f: CheckForm) => {
      if (!demo || !f.agent) return
      setChecking(true)
      setCheckError(null)
      try {
        const decision = await api.authorize({
          subject: f.agent,
          action: "transfer",
          resource: demo.resource,
          amount: Number(f.amount),
          recipient: f.recipient.trim(),
        })
        setResult({ decision, agent: f.agent })
        setLastDecision(decision)
      } catch (e) {
        setCheckError(e instanceof Error ? e.message : String(e))
      } finally {
        setChecking(false)
      }
    },
    [demo],
  )

  const presets: Preset[] = useMemo(() => {
    if (!demo) return []
    const by = (name: string) => demo.principals.find((p) => p.name === name)?.address
    const invoice = by("invoice-agent")
    const legacy = by("legacy-bot")
    return [
      ...(invoice
        ? [
            { label: "$200 to AWS", agent: invoice, amount: 200, recipient: "AWS" },
            { label: "$700 to AWS", agent: invoice, amount: 700, recipient: "AWS" },
            { label: "$100 to RandomVendor", agent: invoice, amount: 100, recipient: "RandomVendor" },
          ]
        : []),
      ...(legacy ? [{ label: "LegacyBot, $50 to AWS", agent: legacy, amount: 50, recipient: "AWS" }] : []),
    ]
  }, [demo])

  const onArkiv = demo?.backend === "arkiv"
  const cut = lastDecision?.decision === "DENY" ? (lastDecision.revokedMandateId ?? null) : null

  return (
    <>
      <Header live={live.status} />
      <main className="page">
        <section className="intro">
          <h1>Permission slips for AI agents, enforced on Arkiv.</h1>
          <p>
            Alice lets FinanceAgent spend from her wallet, and FinanceAgent passes a smaller slice to InvoiceAgent. Every
            grant and revocation is signed and stored on Arkiv, and every check reads it from there.
          </p>
          {demo && (
            <p className="muted small">
              {onArkiv ? "Reading from Arkiv's Tiramisu testnet" : `Backend: ${demo.backend} (not Arkiv)`}
              {onArkiv && (
                <>
                  {" · "}
                  <a href={demo.explorer} target="_blank" rel="noopener">
                    explorer ↗
                  </a>
                </>
              )}
            </p>
          )}
        </section>

        {fatal ? (
          <p className="error">{fatal}</p>
        ) : (
          <>
            <Step n={1} title="Who can act for Alice">
              <p className="muted">
                Each line is a signed mandate. Authority only narrows as it's passed down. Open one to see its evidence on Arkiv.
              </p>
              <Delegations
                edges={edges}
                loading={loading}
                error={treeError}
                nameOf={nameOf}
                explorer={demo?.explorer ?? ""}
                path={lastDecision?.delegationPath ?? []}
                cut={cut}
                selected={selected}
                onSelect={setSelected}
                refreshKey={refreshKey}
              />
            </Step>

            <Step n={2} title="Can it do this?">
              <p className="muted">What a payment tool asks Mandate before it moves money.</p>
              <Check
                agents={agents}
                form={form}
                onChange={setForm}
                onCheck={(f) => void runCheck(f)}
                presets={presets}
                checking={checking}
                result={result}
                error={checkError}
                nameOf={nameOf}
              />
            </Step>

            {demo && (
              <Step n={3} title="Revoke it live">
                <p className="muted">A fresh agent, start to finish. Each step writes to Arkiv or reads from it.</p>
                <RevokeLive
                  resource={demo.resource}
                  maxAmount={demo.sandbox.maxAmount}
                  lifetimeSeconds={demo.sandbox.lifetimeSeconds}
                  explorer={demo.explorer}
                  nameOf={nameOf}
                  onAgent={(p) => {
                    addName(p)
                    setExtraAgents((list) => [...list, p])
                  }}
                  onChanged={() => void loadTree()}
                  onDecision={setLastDecision}
                />
              </Step>
            )}

            <section className="card">
              <h2>Live on Arkiv</h2>
              <LiveFeed status={live.status} events={live.events} explorer={demo?.explorer ?? ""} nameOf={nameOf} />
            </section>
          </>
        )}
      </main>
      <footer className="footer">
        <span>MIT licensed · built for Arkiv: Global Tour Stop</span>
        <span>
          <a href={`${REPO_URL}/blob/main/arkiv/schema.md`} target="_blank" rel="noopener">
            Arkiv schema
          </a>
          <a href={`${REPO_URL}/blob/main/docs/security.md`} target="_blank" rel="noopener">
            Security model
          </a>
          <a href={REPO_URL} target="_blank" rel="noopener">
            GitHub
          </a>
        </span>
      </footer>
    </>
  )
}

function Step({ n, title, children }: { n: number; title: string; children: React.ReactNode }) {
  return (
    <section className="card step">
      <h2>
        <span className="step-num">{n}</span>
        {title}
      </h2>
      {children}
    </section>
  )
}
