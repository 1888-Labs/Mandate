import { useCallback, useEffect, useMemo, useRef, useState } from "react"

import { api } from "./api"
import { GraphCanvas } from "./components/GraphCanvas"
import { Header } from "./components/Header"
import { Hero } from "./components/Hero"
import { HowItWorks } from "./components/HowItWorks"
import { Inspector } from "./components/Inspector"
import { Sandbox, type SandboxItem } from "./components/Sandbox"
import { type Preset, Tester, type TesterForm } from "./components/Tester"
import { Timeline } from "./components/Timeline"
import { Toaster } from "./components/Toaster"
import { DOCS_URL, REPO_URL } from "./brand"
import { displayName, explorerUrl, short } from "./format"
import { useLiveEvents } from "./hooks/useLiveEvents"
import { useToasts } from "./hooks/useToasts"
import type { Decision, DemoInfo, Hex, MandateStatus, Principal } from "./types"

export function App() {
  const [demo, setDemo] = useState<DemoInfo | null>(null)
  const [fatal, setFatal] = useState<string | null>(null)
  const [names, setNames] = useState<Map<string, string>>(new Map())
  const [sandbox, setSandbox] = useState<SandboxItem[]>([])
  const [creating, setCreating] = useState(false)
  const [sandboxError, setSandboxError] = useState<string | null>(null)

  const [edges, setEdges] = useState<MandateStatus[]>([])
  const [graphLoading, setGraphLoading] = useState(true)
  const [graphError, setGraphError] = useState<string | null>(null)

  const [form, setForm] = useState<TesterForm>({ agent: "", amount: "200", recipient: "AWS" })
  const [decision, setDecision] = useState<{ result: Decision; agent: Hex } | null>(null)
  const [selectedId, setSelectedId] = useState<Hex | null>(null)
  const [inspectKey, setInspectKey] = useState(0)
  const [checking, setChecking] = useState(false)
  const [checkError, setCheckError] = useState<string | null>(null)
  const [pulse, setPulse] = useState(0)
  const { toasts, push: toast, dismiss } = useToasts()

  const nameOf = useCallback((address: string) => names.get(address.toLowerCase()) ?? short(address), [names])

  // Agents the graph and the tester know about: the demo principals and any sandbox agents.
  const agents: Principal[] = useMemo(() => {
    if (!demo) return []
    const sandboxAgents = sandbox.flatMap((s) => (s.address ? [{ name: s.name, address: s.address }] : []))
    return [...demo.principals.filter((p) => p.name !== "alice"), ...sandboxAgents]
  }, [demo, sandbox])

  const loadGraph = useCallback(async () => {
    if (agents.length === 0) return
    setGraphLoading(true)
    try {
      const lists = await Promise.all(agents.map((a) => api.agentMandates(a.address).catch(() => [])))
      // One edge per (issuer, subject): the most recently issued mandate.
      const latest = new Map<string, MandateStatus>()
      for (const entry of lists.flat()) {
        const key = `${entry.mandate.issuer}>${entry.mandate.subject}`
        const seen = latest.get(key)
        if (!seen || seen.mandate.issuedAt < entry.mandate.issuedAt) latest.set(key, entry)
      }
      const list = [...latest.values()]
      setEdges(list)
      setGraphError(null)
      setInspectKey((k) => k + 1)
      // Start by inspecting InvoiceAgent's mandate, the heart of the demo.
      setSelectedId((current) => {
        if (current && list.some((e) => e.mandate.id === current)) return current
        const invoice = demo?.principals.find((p) => p.name === "invoice-agent")?.address
        return (list.find((e) => e.mandate.subject === invoice) ?? list[0])?.mandate.id ?? null
      })
    } catch (e) {
      setGraphError(e instanceof Error ? e.message : String(e))
    } finally {
      setGraphLoading(false)
    }
  }, [agents, demo])

  // Load the demo once.
  useEffect(() => {
    api
      .demo()
      .then((d) => {
        setDemo(d)
        setNames(new Map([...d.principals, ...d.sandboxAgents].map((p) => [p.address.toLowerCase(), displayName(p.name)])))
        setSandbox(d.sandboxAgents.map((s) => ({ ...s, mandateId: null, phase: "active", working: null, error: null })))
        const invoice = d.principals.find((p) => p.name === "invoice-agent")
        if (invoice) setForm((f) => ({ ...f, agent: invoice.address }))
      })
      .catch((e: unknown) => {
        const message = e instanceof Error ? e.message : String(e)
        // 502/504 from the dev proxy, or a network error: the API isn't running.
        const unreachable = /^50[24]\b|Failed to fetch|NetworkError/.test(message)
        setFatal(
          unreachable
            ? "can't reach the Mandate API. Start it with scripts/run-local.sh (it serves on port 8080), then reload."
            : message,
        )
      })
  }, [])

  useEffect(() => {
    void loadGraph()
  }, [loadGraph])

  // Live events from Arkiv: refresh the graph, debounced.
  const refreshTimer = useRef<ReturnType<typeof setTimeout>>(undefined)
  const live = useLiveEvents(Boolean(demo?.liveEvents), () => {
    setPulse((n) => n + 1)
    clearTimeout(refreshTimer.current)
    refreshTimer.current = setTimeout(() => void loadGraph(), 400)
  })

  const runCheck = useCallback(
    async (f: TesterForm) => {
      if (!demo || !f.agent) return
      setChecking(true)
      setCheckError(null)
      try {
        const result = await api.authorize({
          subject: f.agent,
          action: "transfer",
          resource: demo.resource,
          amount: Number(f.amount),
          recipient: f.recipient.trim(),
        })
        setDecision({ result, agent: f.agent })
        // Point the inspector at the mandate that decided it.
        const decisive = result.decision === "DENY" ? (result.revokedMandateId ?? result.mandateId) : result.mandateId
        if (decisive) setSelectedId(decisive)
      } catch (e) {
        setCheckError(e instanceof Error ? e.message : String(e))
      } finally {
        setChecking(false)
      }
    },
    [demo],
  )

  const checkAgent = useCallback(
    (agent: Hex, amount = 10) => {
      const f = { agent, amount: String(amount), recipient: "AWS" }
      setForm(f)
      void runCheck(f)
      document.getElementById("check")?.scrollIntoView({ behavior: "smooth", block: "center" })
    },
    [runCheck],
  )

  const createSandbox = async () => {
    setCreating(true)
    setSandboxError(null)
    const pending: SandboxItem = {
      name: "sandbox agent",
      address: null,
      mandateId: null,
      phase: "pending",
      working: "FinanceAgent is signing and publishing to Arkiv…",
      error: null,
    }
    setSandbox((list) => [pending, ...list])
    try {
      const r = await api.createSandbox()
      setNames((n) => new Map(n).set(r.agent.address.toLowerCase(), displayName(r.agent.name)))
      setSandbox((list) =>
        list.map((s) =>
          s === pending ? { ...s, name: r.agent.name, address: r.agent.address, mandateId: r.mandate.id, phase: "active", working: null } : s,
        ),
      )
      toast({
        tone: "ok",
        title: "Mandate published to Arkiv",
        detail: `FinanceAgent → ${displayName(r.agent.name)}, signed by FinanceAgent's wallet`,
        href: explorerUrl(demo?.explorer ?? "", "address", r.mandate.issuer) ?? undefined,
      })
      checkAgent(r.agent.address)
    } catch (e) {
      setSandbox((list) => list.filter((s) => s !== pending))
      const message = e instanceof Error ? e.message : String(e)
      setSandboxError(message)
      toast({ tone: "bad", title: "Couldn't create a sandbox agent", detail: message })
    } finally {
      setCreating(false)
    }
  }

  const revokeSandbox = async (item: SandboxItem) => {
    if (!item.mandateId || !item.address) return
    const update = (patch: Partial<SandboxItem>) =>
      setSandbox((list) => list.map((s) => (s.address === item.address ? { ...s, ...patch } : s)))
    update({ working: "FinanceAgent is signing and publishing the revocation…", error: null })
    try {
      await api.revokeSandbox(item.mandateId)
      update({ phase: "revoked", working: null })
      toast({ tone: "info", title: "Revoked on Arkiv", detail: `${nameOf(item.address)} loses its authority on the next check.` })
      await loadGraph()
      checkAgent(item.address)
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      update({ working: null, error: message })
      toast({ tone: "bad", title: "Revocation failed", detail: message })
    }
  }

  // A sandbox card's status and mandate come from the live graph, not memory:
  // after a reload, a revoked sandbox must still show as revoked.
  const sandboxView: SandboxItem[] = useMemo(
    () =>
      sandbox.map((s) => {
        const e = edges.find((x) => s.address && x.mandate.subject === s.address)
        if (!e || s.working) return s
        return { ...s, mandateId: s.mandateId ?? e.mandate.id, phase: e.status === "ACTIVE" ? "active" : "revoked" }
      }),
    [sandbox, edges],
  )

  const presets: Preset[] = useMemo(() => {
    if (!demo) return []
    const by = (name: string) => demo.principals.find((p) => p.name === name)?.address
    const invoice = by("invoice-agent")
    const legacy = by("legacy-bot")
    return [
      ...(invoice
        ? [
            { label: "$200 → AWS", agent: invoice, amount: 200, recipient: "AWS" },
            { label: "$700 → AWS", agent: invoice, amount: 700, recipient: "AWS" },
            { label: "$100 → RandomVendor", agent: invoice, amount: 100, recipient: "RandomVendor" },
          ]
        : []),
      ...(legacy ? [{ label: "LegacyBot $50", agent: legacy, amount: 50, recipient: "AWS" }] : []),
    ]
  }, [demo])

  return (
    <>
      <Header live={live.status} pulse={pulse} />
      <Hero
        mandates={edges.length}
        revoked={edges.filter((e) => e.status !== "ACTIVE").length}
        network="Tiramisu testnet"
      />
      <HowItWorks />

      {fatal ? (
        <main className="layout">
          <section className="card wide">
            <p className="error">Demo mode is not available: {fatal}</p>
          </section>
        </main>
      ) : (
        <main className="layout">
          <section className="card graph-card" id="graph">
            <div className="card-head">
              <div>
                <h2>Delegation graph</h2>
                <p className="hint">Live from Arkiv. Each arrow is a signed mandate; authority only narrows going down. Click one to inspect it.</p>
              </div>
              <button className="ghost icon" onClick={() => void loadGraph()} title="Reload from Arkiv" disabled={graphLoading}>
                ↻
              </button>
            </div>
            {graphError ? (
              <p className="error">{graphError}</p>
            ) : edges.length === 0 ? (
              <div className="graph-canvas empty">
                {graphLoading ? <div className="skeleton-graph" aria-label="Loading from Arkiv" /> : "No active mandates on Arkiv."}
              </div>
            ) : (
              <GraphCanvas
                edges={edges}
                nameOf={nameOf}
                highlight={decision?.result.delegationPath ?? []}
                cutMandate={decision?.result.decision === "DENY" ? (decision.result.revokedMandateId ?? null) : null}
                selected={selectedId}
                onSelect={(e) => setSelectedId(e.mandate.id)}
              />
            )}
            <div className="legend">
              <span><i className="swatch active" /> active</span>
              <span><i className="swatch revoked" /> revoked (cut)</span>
              <span><i className="swatch path" /> last decision's path</span>
            </div>
          </section>

          <Tester
            agents={agents}
            resourceLabel={demo ? `${demo.resource.kind}:${demo.resource.id}` : "…"}
            form={form}
            onChange={setForm}
            onCheck={() => void runCheck(form)}
            onPreset={(p) => {
              const f = { agent: p.agent, amount: String(p.amount), recipient: p.recipient }
              setForm(f)
              void runCheck(f)
            }}
            checking={checking}
            decision={decision}
            error={checkError}
            nameOf={nameOf}
            presets={presets}
            onInspect={(id) => setSelectedId(id)}
          />

          <Inspector
            entry={edges.find((e) => e.mandate.id === selectedId) ?? null}
            nameOf={nameOf}
            explorer={demo?.explorer ?? ""}
            refreshKey={inspectKey}
            onCheck={(agent) => checkAgent(agent, 10)}
          />
          <Sandbox
            items={sandboxView}
            creating={creating}
            error={sandboxError}
            maxAmount={demo?.sandbox.maxAmount ?? 50}
            onCreate={() => void createSandbox()}
            onRevoke={(s) => void revokeSandbox(s)}
            onCheck={(agent) => checkAgent(agent)}
            nameOf={nameOf}
          />
          <Timeline events={live.events} explorer={demo?.explorer ?? ""} nameOf={nameOf} />
        </main>
      )}

      <footer className="footer">
        <div className="footer-brand">
          <span className="wordmark">MANDATE</span>
          <p className="muted">Authorization for AI agents. Signed, scoped, revocable, and checked live on Arkiv.</p>
          <span className="muted small">Built for Arkiv: Global Tour Stop · MIT licensed</span>
        </div>
        <div className="footer-col">
          <b>Product</b>
          <a href={DOCS_URL} target="_blank" rel="noopener">Documentation</a>
          <a href={`${REPO_URL}/blob/main/docs/api.md`} target="_blank" rel="noopener">HTTP API</a>
          <a href={`${REPO_URL}/blob/main/docs/security.md`} target="_blank" rel="noopener">Security model</a>
        </div>
        <div className="footer-col">
          <b>Arkiv</b>
          <a href={`${REPO_URL}/blob/main/arkiv/schema.md`} target="_blank" rel="noopener">Entity schema</a>
          <a href={`${REPO_URL}/blob/main/arkiv/demo-dataset.json`} target="_blank" rel="noopener">Dataset evidence</a>
          {demo?.explorer && <a href={demo.explorer} target="_blank" rel="noopener">Explorer ↗</a>}
        </div>
        <div className="footer-col">
          <b>Project</b>
          <a href={REPO_URL} target="_blank" rel="noopener">GitHub</a>
          <a href={`${REPO_URL}#readme`} target="_blank" rel="noopener">README</a>
        </div>
      </footer>
      <Toaster toasts={toasts} onDismiss={dismiss} />
    </>
  )
}
