// BUILD_SPEC §35: "expired data disappears from live queries", on Tiramisu.
//
// Publishes a mandate that expires in 3 minutes, checks it is queryable,
// waits past its expiry, and checks it is gone. Takes about 4 minutes.
//
//   cargo build -p mandate-cli && npm run test:arkiv-expiry

import assert from "node:assert/strict"
import { execFileSync } from "node:child_process"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { setTimeout as sleep } from "node:timers/promises"
import { fileURLToPath } from "node:url"
import type { Hex } from "viem"

import { connect } from "../src/arkiv.ts"
import { parseMandate } from "../src/documents.ts"

const privateKey = process.env.ARKIV_PRIVATE_KEY as Hex | undefined
if (!privateKey) throw new Error("set ARKIV_PRIVATE_KEY (see .env)")
const projectId = `${process.env.ARKIV_PROJECT_ID ?? "mandate-global-tour-2026"}-acceptance`

const repo = fileURLToPath(new URL("../../../", import.meta.url))
const home = mkdtempSync(join(tmpdir(), "mandate-expiry-"))
const cli = (...args: string[]) =>
  execFileSync(join(repo, "target/debug/mandate"), args, {
    env: { ...process.env, MANDATE_HOME: home },
    encoding: "utf8",
  })
for (const name of ["alice", "bot"]) cli("principal", "create", name)
cli("resource", "own", "wallet:expiry-test", "--owner", "alice")
cli(
  "grant", "--from", "alice", "--to", "bot", "--action", "transfer",
  "--resource", "wallet:expiry-test", "--expires-in", "3m",
)
const raw = cli("export", "M1").trim()
rmSync(home, { recursive: true, force: true })
const mandate = parseMandate(JSON.parse(raw))

const arkiv = connect({ projectId, rpcUrl: process.env.ARKIV_RPC_URL || undefined, privateKey })
const { entityKey } = await arkiv.createMandate(raw, mandate)
console.log(`published ${mandate.id} as ${entityKey}, expires ${new Date(mandate.expiresAt * 1000).toISOString()}`)

assert.equal((await arkiv.mandatesById(mandate.id)).length, 1, "must be queryable before expiry")
console.log("✓ queryable before expiry")

// Arkiv expiry is block-based; allow a little slack past the signed expiry.
const deadline = mandate.expiresAt + 90
while (Math.floor(Date.now() / 1000) < deadline) {
  const found = (await arkiv.mandatesById(mandate.id)).length
  const left = mandate.expiresAt - Math.floor(Date.now() / 1000)
  console.log(`  ${left > 0 ? `${left}s to expiry` : `${-left}s past expiry`}: ${found} found`)
  if (found === 0) break
  await sleep(20_000)
}
const after = await arkiv.mandatesById(mandate.id)
assert.equal(after.length, 0, "an expired mandate must disappear from live queries")
const lag = Math.floor(Date.now() / 1000) - mandate.expiresAt
console.log(`✓ gone from live queries (${lag >= 0 ? `${lag}s after` : `${-lag}s before`} its signed expiry)`)
