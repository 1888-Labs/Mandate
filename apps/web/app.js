// Mandate web UI. Plain JavaScript, no build step.
//
// Everything shown comes from the Mandate API, which reads it from Arkiv.
// Documents on Arkiv can be written by anyone, so nothing is ever inserted as
// HTML: every value goes through text nodes (see `el`).

const HEX64 = /^0x[0-9a-fA-F]{64}$/
const ADDR = /^0x[0-9a-fA-F]{40}$/
const COLORS = ["#8b7bff", "#22c55e", "#0ea5e9", "#f59e0b", "#ec4899", "#14b8a6", "#a855f7"]

const state = {
  demo: null,
  names: new Map(), // lowercase address → display name
  explorer: "https://tiramisu.explorer.arkiv.network",
  sandbox: [], // { name, address, mandateId, phase, error }
}

// ---------- helpers ----------

/** Create an element. Strings become text nodes, never HTML. */
function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag)
  for (const [k, v] of Object.entries(attrs)) {
    if (v === false || v === null || v === undefined) continue
    if (k === "class") node.className = v
    else if (k.startsWith("on")) node.addEventListener(k.slice(2), v)
    else node.setAttribute(k, v === true ? "" : String(v))
  }
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue
    node.append(c instanceof Node ? c : document.createTextNode(String(c)))
  }
  return node
}

const $ = (id) => document.getElementById(id)
const short = (hex) => (typeof hex === "string" && hex.length > 14 ? `${hex.slice(0, 6)}…${hex.slice(-4)}` : hex)
const nameOf = (addr) => state.names.get(String(addr).toLowerCase()) ?? short(addr)

/** "finance-agent" → "FinanceAgent", "sandbox-ab12" → "Sandbox ab12": the names the docs use. */
function display(name) {
  if (name.startsWith("sandbox-")) return `Sandbox ${name.slice(8)}`
  return name.split("-").map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join("")
}
const money = (n) => `$${Number(n).toLocaleString()}`

function explorerLink(kind, value, label) {
  const ok = kind === "address" ? ADDR.test(value) : HEX64.test(value)
  if (!ok) return null
  return el("a", { href: `${state.explorer}/${kind}/${value}`, target: "_blank", rel: "noopener" }, label)
}

function avatar(name) {
  let h = 0
  for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) >>> 0
  return el("span", { class: "avatar", style: `background:${COLORS[h % COLORS.length]}` }, name.slice(0, 1).toUpperCase())
}

function limits(c) {
  const amount = c?.maxAmount == null ? "any amount" : `≤ ${money(c.maxAmount)}`
  const to = c?.allowedRecipients == null ? "anyone" : c.allowedRecipients.length ? c.allowedRecipients.join(", ") : "no one"
  return `${amount} → ${to}`
}

function relative(unix) {
  const s = unix - Date.now() / 1000
  const abs = Math.abs(s)
  const span = abs >= 86400 ? `${Math.round(abs / 86400)}d` : abs >= 3600 ? `${Math.round(abs / 3600)}h` : `${Math.max(1, Math.round(abs / 60))}m`
  return s >= 0 ? `expires in ${span}` : `expired ${span} ago`
}

async function api(path, options = {}) {
  const res = await fetch(path, { headers: { "content-type": "application/json" }, ...options })
  const body = await res.json().catch(() => ({}))
  if (!res.ok && !("decision" in body)) {
    throw new Error(body.message ?? `${res.status} ${res.statusText}`)
  }
  return body
}

// ---------- graph ----------

function statusOf(entry) {
  const s = entry.status
  if (s === "ACTIVE") return ["active", "ACTIVE"]
  if (s === "REVOKED" || s === "ANCESTOR_REVOKED") return ["revoked", s]
  if (s === "EXPIRED") return ["expired", "EXPIRED"]
  return ["denied", s]
}

async function loadGraph() {
  const graph = $("graph")
  const agents = [
    ...state.demo.principals.filter((p) => p.name !== "alice"),
    ...state.sandbox.filter((s) => s.address).map((s) => ({ name: s.name, address: s.address })),
  ]
  const lists = await Promise.all(
    agents.map((a) => api(`/v1/agents/${a.address}/mandates?active=true`).catch(() => ({ mandates: [] }))),
  )
  // One edge per (issuer, subject): the most recently issued mandate.
  const edges = new Map()
  for (const list of lists) {
    for (const entry of list.mandates ?? []) {
      const m = entry.mandate
      const key = `${m.issuer}>${m.subject}`
      if (!edges.has(key) || edges.get(key).mandate.issuedAt < m.issuedAt) edges.set(key, entry)
    }
  }
  const all = [...edges.values()]
  const subjects = new Set(all.map((e) => e.mandate.subject))
  const roots = [...new Set(all.map((e) => e.mandate.issuer))].filter((i) => !subjects.has(i))

  const renderNode = (address, depth) => {
    const name = nameOf(address)
    const children = all.filter((e) => e.mandate.issuer === address).sort((a, b) => a.mandate.issuedAt - b.mandate.issuedAt)
    const role = depth === 0 ? "trust anchor · owns the treasury wallet" : null
    return el(
      "div",
      { class: "tree" },
      el(
        "div",
        { class: "node" },
        avatar(name),
        el("div", { class: "who" }, el("b", {}, name), role ? el("span", { class: "small muted" }, role) : el("span", { class: "mono small" }, short(address))),
      ),
      children.length
        ? el(
            "div",
            { class: "children" },
            children.map((entry) => el("div", { class: "branch" }, renderEdge(entry), depth < 8 ? renderNode(entry.mandate.subject, depth + 1) : null)),
          )
        : null,
    )
  }

  graph.replaceChildren(...(roots.length ? roots.map((r) => renderNode(r, 0)) : [el("p", { class: "muted" }, "No active mandates found on Arkiv.")]))
}

function renderEdge(entry) {
  const m = entry.mandate
  const [cls, label] = statusOf(entry)
  const creator = entry.arkiv?.creator
  const byIssuer = creator && creator.toLowerCase() === m.issuer.toLowerCase()
  return el(
    "div",
    { class: `edge ${cls}`, title: "Check this agent", onclick: () => selectAgent(m.subject) },
    el(
      "div",
      { class: "edge-top" },
      el("span", { class: "limits" }, `${m.action} ${limits(m.constraints)}`),
      el("span", { class: `status ${cls}` }, label),
    ),
    el(
      "div",
      { class: "meta" },
      el("span", {}, `${nameOf(m.issuer)} → ${nameOf(m.subject)}`),
      m.delegable ? el("span", {}, "may delegate") : null,
      el("span", {}, relative(m.expiresAt)),
      creator
        ? el("span", { class: byIssuer ? "verified" : "unverified" }, byIssuer ? `✓ published by ${nameOf(creator)} ($creator)` : `✗ creator ${short(creator)} is not the issuer`)
        : null,
      entry.arkiv?.entityKey ? explorerLink("entity", entry.arkiv.entityKey, "Arkiv entity ↗") : null,
    ),
  )
}

// ---------- tester ----------

function fillAgents() {
  const select = $("agent")
  const current = select.value
  const options = [
    ...state.demo.principals.filter((p) => p.name !== "alice"),
    ...state.sandbox.filter((s) => s.address).map((s) => ({ name: s.name, address: s.address })),
  ]
  select.replaceChildren(...options.map((p) => el("option", { value: p.address }, display(p.name))))
  if (options.some((p) => p.address === current)) select.value = current
}

function selectAgent(address) {
  $("agent").value = address
  $("agent").scrollIntoView({ behavior: "smooth", block: "center" })
}

async function check(event) {
  event?.preventDefault()
  const button = $("check")
  const box = $("result")
  button.disabled = true
  try {
    const decision = await api("/v1/authorize", {
      method: "POST",
      body: JSON.stringify({
        subject: $("agent").value,
        action: "transfer",
        resource: state.demo.resource,
        amount: Number($("amount").value),
        recipient: $("recipient").value.trim(),
      }),
    })
    renderDecision(box, decision)
  } catch (e) {
    box.hidden = false
    box.className = "result deny"
    box.replaceChildren(el("div", { class: "error" }, e.message))
  } finally {
    button.disabled = false
  }
}

function renderDecision(box, d) {
  const allow = d.decision === "ALLOW"
  box.hidden = false
  box.className = `result ${allow ? "allow" : "deny"}`
  const path = (d.delegationPath ?? []).flatMap((a, i) => [i ? el("span", { class: "arrow" }, "→") : null, el("span", { class: "pill" }, nameOf(a))])
  const details = []
  if (d.requestedAmount !== undefined) details.push(`requested ${d.requestedAmount === null ? "nothing" : money(d.requestedAmount)}, maximum ${money(d.maximumAmount)}`)
  if (d.allowedRecipients) details.push(`allowed recipients: ${d.allowedRecipients.join(", ") || "none"}`)
  if (d.revokedMandateId) details.push(`revoked mandate ${short(d.revokedMandateId)}`)
  box.replaceChildren(
    el("div", { class: "verdict" }, allow ? "✓ ALLOW" : "✗ DENY", allow ? null : el("span", { class: "code" }, d.reason)),
    allow ? el("p", {}, `Within ${limits(d.effectiveConstraints)}, granted by ${nameOf(d.rootPrincipal)}.`) : el("p", {}, d.message ?? ""),
    details.length ? el("p", { class: "muted" }, details.join(" · ")) : null,
    path.length > 1 ? el("div", { class: "path" }, el("span", { class: "muted" }, "Chain:"), path) : null,
  )
}

// ---------- sandbox ----------

function renderSandbox() {
  $("sandbox").replaceChildren(
    ...state.sandbox.map((s) =>
      el(
        "div",
        { class: "sandbox-item" },
        el("div", { class: "edge-top" }, el("b", {}, display(s.name)), el("span", { class: `status ${s.phase === "revoked" ? "revoked" : "active"}` }, s.phase === "revoked" ? "REVOKED" : "ACTIVE")),
        el("span", { class: "meta" }, `FinanceAgent → ${display(s.name)}: transfer ≤ $50 → AWS`),
        s.working ? el("span", { class: "working" }, el("span", { class: "spinner" }), s.working) : null,
        s.error ? el("span", { class: "error" }, s.error) : null,
        el(
          "div",
          { class: "actions" },
          el("button", { class: "small", onclick: () => quickCheck(s.address, 10) }, "Check $10 → AWS"),
          s.phase !== "revoked"
            ? el("button", { class: "small danger", disabled: Boolean(s.working), onclick: () => revokeSandbox(s) }, "Revoke")
            : null,
        ),
      ),
    ),
  )
}

async function quickCheck(address, amount) {
  fillAgents()
  $("agent").value = address
  $("amount").value = amount
  $("recipient").value = "AWS"
  await check()
  $("result").scrollIntoView({ behavior: "smooth", block: "center" })
}

async function createSandbox() {
  const button = $("sandbox-create")
  button.disabled = true
  const placeholder = { name: "sandbox agent", address: "", phase: "pending", working: "FinanceAgent is signing and publishing to Arkiv…" }
  state.sandbox.unshift(placeholder)
  renderSandbox()
  try {
    const r = await api("/v1/demo/sandbox", { method: "POST" })
    Object.assign(placeholder, { name: r.agent.name, address: r.agent.address, mandateId: r.mandate.id, phase: "active", working: null })
    state.names.set(r.agent.address.toLowerCase(), display(r.agent.name))
    fillAgents()
    await loadGraph()
    await quickCheck(r.agent.address, 10)
  } catch (e) {
    state.sandbox.shift()
    $("sandbox").prepend(el("p", { class: "error" }, e.message))
  } finally {
    button.disabled = false
    renderSandbox()
  }
}

async function revokeSandbox(s) {
  s.working = "FinanceAgent is signing and publishing the revocation…"
  s.error = null
  renderSandbox()
  try {
    await api(`/v1/demo/sandbox/${s.mandateId}/revoke`, { method: "POST" })
    s.phase = "revoked"
    await loadGraph()
    await quickCheck(s.address, 10)
  } catch (e) {
    s.error = e.message
  } finally {
    s.working = null
    renderSandbox()
  }
}

// ---------- live events ----------

function connectEvents() {
  const live = $("live")
  if (!state.demo.liveEvents) {
    $("live-text").textContent = "live events off"
    return
  }
  const source = new EventSource("/v1/events")
  let refresh
  source.addEventListener("ready", () => {
    live.classList.add("on")
    $("live-text").textContent = "live"
  })
  source.addEventListener("arkiv", (msg) => {
    let e
    try {
      e = JSON.parse(msg.data)
    } catch {
      return
    }
    addFeed(e)
    live.classList.remove("flash")
    void live.offsetWidth
    live.classList.add("flash")
    clearTimeout(refresh)
    refresh = setTimeout(() => loadGraph().catch(() => {}), 400)
  })
  source.onerror = () => {
    live.classList.remove("on")
    $("live-text").textContent = "reconnecting…"
  }
}

function addFeed(e) {
  const feed = $("feed")
  feed.querySelector(".empty")?.remove()
  const who = nameOf(e.creator)
  const what =
    e.action === "deleted"
      ? `${who}'s entity was deleted`
      : e.kind === "revocation"
        ? `${who} published a revocation`
        : `${who} published a mandate`
  feed.prepend(
    el(
      "li",
      { class: "new" },
      el("span", { class: "time" }, new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hourCycle: "h23" })),
      el("span", {}, what, el("span", { class: "muted" }, ` · block ${e.blockNumber}`)),
      el("span", { class: "links" }, explorerLink("tx", e.txHash, "tx ↗"), explorerLink("entity", e.entityKey, "entity ↗")),
    ),
  )
  while (feed.children.length > 25) feed.lastElementChild.remove()
}

// ---------- start ----------

async function main() {
  try {
    state.demo = await api("/v1/demo")
  } catch (e) {
    $("graph").replaceChildren(el("p", { class: "error" }, `Demo mode is not available: ${e.message}`))
    return
  }
  state.explorer = state.demo.explorer ?? state.explorer
  for (const p of [...state.demo.principals, ...state.demo.sandboxAgents]) state.names.set(p.address.toLowerCase(), display(p.name))
  state.sandbox = state.demo.sandboxAgents.map((s) => ({ ...s, phase: "active" }))
  $("resource").textContent = `${state.demo.resource.kind}:${state.demo.resource.id}`
  fillAgents()
  const invoice = state.demo.principals.find((p) => p.name === "invoice-agent")
  if (invoice) $("agent").value = invoice.address

  $("tester").addEventListener("submit", check)
  $("refresh").addEventListener("click", () => loadGraph())
  $("sandbox-create").addEventListener("click", createSandbox)
  for (const b of document.querySelectorAll("[data-preset]")) {
    b.addEventListener("click", () => {
      const [name, amount, recipient] = b.dataset.preset.split(",")
      const p = state.demo.principals.find((x) => x.name === name)
      if (!p) return
      $("agent").value = p.address
      $("amount").value = amount
      $("recipient").value = recipient
      check()
    })
  }
  connectEvents()
  renderSandbox()
  await loadGraph().catch((e) => $("graph").replaceChildren(el("p", { class: "error" }, e.message)))
}

main()
