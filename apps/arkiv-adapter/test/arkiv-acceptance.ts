// Milestone 9 acceptance against the real Arkiv Tiramisu testnet.
//
// Uses the Rust CLI to produce real signed documents, publishes them through
// the adapter's HTTP API, and checks every query the engine needs. Writes go
// to a separate `<project>-acceptance` namespace so demo data stays clean.
//
//   cargo build -p mandate-cli && npm run test:arkiv
//
// Needs ARKIV_PRIVATE_KEY (a funded Tiramisu wallet) in .env.

import assert from "node:assert/strict"
import { execFileSync } from "node:child_process"
import { randomBytes } from "node:crypto"
import { mkdtempSync, rmSync } from "node:fs"
import { createServer } from "node:http"
import type { AddressInfo } from "node:net"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { fileURLToPath } from "node:url"

import { createWalletClient } from "@arkiv-network/sdk"
import { str } from "@arkiv-network/sdk/attr"
import { tiramisu } from "@arkiv-network/sdk/chains"
import { type Hex, http } from "viem"
import { privateKeyToAccount } from "viem/accounts"

import { BURN_ADDRESS, connect } from "../src/arkiv.ts"
import { handler } from "../src/server.ts"

const privateKey = process.env.ARKIV_PRIVATE_KEY as Hex | undefined
if (!privateKey) throw new Error("set ARKIV_PRIVATE_KEY (see .env)")
const projectId = `${process.env.ARKIV_PROJECT_ID ?? "mandate-global-tour-2026"}-acceptance`

// --- Signed documents from the Rust CLI ---

const repo = fileURLToPath(new URL("../../../", import.meta.url))
const bin = join(repo, "target/debug/mandate")
const home = mkdtempSync(join(tmpdir(), "mandate-arkiv-"))
const cli = (...args: string[]) =>
  execFileSync(bin, args, { env: { ...process.env, MANDATE_HOME: home }, encoding: "utf8" })

for (const name of ["alice", "finance-agent", "invoice-agent"]) cli("principal", "create", name)
cli("resource", "own", "wallet:treasury-demo", "--owner", "alice")
cli(
  "grant", "--from", "alice", "--to", "finance-agent", "--action", "transfer",
  "--resource", "wallet:treasury-demo", "--max-amount", "1000", "--recipient", "AWS",
  "--delegable", "--expires-in", "2h",
)
cli(
  "delegate", "--mandate", "M1", "--from", "finance-agent", "--to", "invoice-agent",
  "--max-amount", "300", "--expires-in", "1h",
)
cli("revoke", "M1")
const m1Raw = cli("export", "M1").trim()
const m2Raw = cli("export", "M2").trim()
const revocationRaw = cli("export", "M1", "--revocations").trim().slice(1, -1).trim() // unwrap [ ]
rmSync(home, { recursive: true, force: true })

const m1 = JSON.parse(m1Raw)
const m2 = JSON.parse(m2Raw)
const revocation = JSON.parse(revocationRaw)

// --- Adapter in-process, against Tiramisu ---

const arkiv = connect({ projectId, rpcUrl: process.env.ARKIV_RPC_URL || undefined, privateKey })
const token = randomBytes(16).toString("hex")
const server = createServer(handler(arkiv, token))
await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve))
const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`

const post = async (path: string, body: string) => {
  const res = await fetch(`${base}${path}`, {
    method: "POST",
    body,
    headers: { authorization: `Bearer ${token}` },
  })
  return { status: res.status, json: await res.json() }
}
const get = async (path: string) => {
  const res = await fetch(`${base}${path}`)
  const text = await res.text()
  assert.equal(res.status, 200, `${path}: ${text}`)
  return text
}

let step = 0
const ok = (what: string) => console.log(`✓ ${++step}. ${what}`)

try {
  console.log(`namespace ${projectId}, writer ${arkiv.writer}\n`)

  const created1 = await post("/v1/mandates", m1Raw)
  assert.equal(created1.status, 201, JSON.stringify(created1.json))
  ok(`create M1 → entity ${created1.json.entityKey}`)

  const byId = await get(`/v1/mandates/${m1.id}`)
  assert.ok(byId.includes(m1Raw), "M1 must come back byte-for-byte")
  ok("query M1 by ID (returned byte-for-byte)")

  const created2 = await post("/v1/mandates", m2Raw)
  assert.equal(created2.status, 201, JSON.stringify(created2.json))
  ok(`create M2 → entity ${created2.json.entityKey}`)

  const bySubject = await get(
    `/v1/mandates?subject=${m2.subject}&action=transfer&resource=wallet:treasury-demo`,
  )
  assert.ok(bySubject.includes(m2Raw), "M2 must be found by subject + action + resource")
  const wrongResource = await get(`/v1/mandates?subject=${m2.subject}&resource=wallet:other`)
  assert.ok(!wrongResource.includes(m2.id), "resource filter must exclude M2")
  ok("query M2 by subject + action + resource (and not by another resource)")

  const children = await get(`/v1/mandates?parent=${m1.id}`)
  assert.ok(children.includes(m2Raw), "M2 must be a child of M1")
  ok("query children of M1")

  const createdR = await post(`/v1/revocations?retain_until=${m1.expiresAt}`, revocationRaw)
  assert.equal(createdR.status, 201, JSON.stringify(createdR.json))
  ok(`create revocation of M1 → entity ${createdR.json.entityKey}`)

  const revs = JSON.parse(await get(`/v1/revocations?mandate=${m1.id}`))
  const stored = revs.documents.find((d: { document: { id: string } }) => d.document.id === revocation.id)
  assert.ok(stored, "revocation must be found by mandate ID")
  assert.ok((await get(`/v1/revocations?mandate=${m1.id}`)).includes(revocationRaw))
  assert.equal(stored.owner.toLowerCase(), BURN_ADDRESS.toLowerCase())
  ok("query revocation of M1 (owned by the burn address)")

  const again = await post("/v1/mandates", m1Raw)
  assert.equal(again.status, 200)
  assert.equal(again.json.created, false)
  ok("re-publishing M1 is a no-op")

  const wallet = createWalletClient({
    chain: tiramisu,
    transport: http(process.env.ARKIV_RPC_URL || undefined),
    account: privateKeyToAccount(privateKey),
  })
  await assert.rejects(
    wallet.deleteEntity({ entityKey: createdR.json.entityKey }),
    "the adapter wallet must not be able to delete a revocation",
  )
  ok("adapter wallet cannot delete the revocation")

  await assert.rejects(
    wallet.patchEntity({ entityKey: created1.json.entityKey, set: { tampered: str("yes") } }),
    "a read-only mandate entity must not be patchable",
  )
  ok("mandate entity is read-only (patch rejected)")

  console.log("\nMilestone 9 acceptance: all checks passed.")
} finally {
  server.close()
}
