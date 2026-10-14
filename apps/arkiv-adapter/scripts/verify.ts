// Read-only check of Mandate's public dataset on Arkiv Tiramisu. No private
// key needed: it lists every mandate and revocation in the namespace with its
// on-chain $creator and $owner, and whether $creator is the issuer named in
// the signed payload (the only entities the engine accepts).
//
//   npm run verify                       # namespace mandate-global-tour-2026
//   ARKIV_PROJECT_ID=… npm run verify    # another namespace
//
// ARKIV_API_KEY is optional: it is sent as X-API-KEY, as the adapter does, so
// the query is charged to your key instead of the small anonymous budget.

import { readFileSync } from "node:fs"
import { setDefaultAutoSelectFamilyAttemptTimeout } from "node:net"

import { createPublicClient } from "@arkiv-network/sdk"
import { str } from "@arkiv-network/sdk/attr"
import { tiramisu } from "@arkiv-network/sdk/chains"
import { and, eq } from "@arkiv-network/sdk/query"
import { http } from "viem"

const PROJECT_ID = process.env.ARKIV_PROJECT_ID || "mandate-global-tour-2026"
const BURN_ADDRESS = "0x000000000000000000000000000000000000dead"
const MAX_RESULTS = 1000

// Same slow-link fix as the adapter (arkiv/friction.md #5).
setDefaultAutoSelectFamilyAttemptTimeout(2_000)

const apiKey = process.env.ARKIV_API_KEY
const client = createPublicClient({
  chain: tiramisu,
  transport: http(process.env.ARKIV_RPC_URL || undefined, {
    timeout: 30_000,
    fetchOptions: apiKey ? { headers: { "X-API-KEY": apiKey } } : {},
  }),
})

// Entity keys recorded by scripts/seed-arkiv.sh, to mark the demo dataset.
const dataset: {
  mandates: { entityKey: string }[]
  revocations: { entityKey: string }[]
} = JSON.parse(readFileSync(new URL("../../../arkiv/demo-dataset.json", import.meta.url), "utf8"))
const demoKeys = new Set(
  [...dataset.mandates, ...dataset.revocations].map((e) => e.entityKey.toLowerCase()),
)

const utc = (seconds: unknown) =>
  typeof seconds === "number" ? new Date(seconds * 1000).toISOString().replace(".000", "") : "?"

console.log(`chain ${await client.getChainId()}, namespace project_id = "${PROJECT_ID}"\n`)

let mismatched = 0
for (const entityType of ["mandate", "revocation"] as const) {
  const query = client
    .select({ key: true, owner: true, creator: true, payload: true, expiresAt: true })
    .where(and(eq("project_id", str(PROJECT_ID)), eq("entity_type", str(entityType))))
    .limit(200)
  let count = 0
  for await (const e of query) {
    if (++count > MAX_RESULTS) {
      console.log(`… more than ${MAX_RESULTS} ${entityType} entities, stopping`)
      break
    }
    let doc: Record<string, unknown>
    try {
      doc = JSON.parse(e.toText())
    } catch {
      console.log(`${entityType} ${e.key}: payload is not JSON (ignored by Mandate)\n`)
      continue
    }
    // JSON.parse rounds the u64 nonce; only string and small-number fields are read here.
    const issuer = typeof doc.issuer === "string" ? doc.issuer : "?"
    const byIssuer = issuer.toLowerCase() === e.creator.toLowerCase()
    if (!byIssuer) mismatched++
    const burned = e.owner.toLowerCase() === BURN_ADDRESS
    console.log(
      [
        `${entityType}${demoKeys.has(e.key.toLowerCase()) ? "  (in arkiv/demo-dataset.json)" : ""}`,
        `  entity key   ${e.key}`,
        `  mandate id   ${entityType === "mandate" ? doc.id : doc.mandateId}`,
        `  issuer       ${issuer}`,
        `  $creator     ${e.creator}`,
        `  $owner       ${e.owner}${burned ? "  (burn address: nobody can delete it)" : ""}`,
        entityType === "mandate"
          ? `  expires_at   ${utc(doc.expiresAt)} (signed), entity expires at block ${e.expiresAt}`
          : `  revoked_at   ${utc(doc.revokedAt)}, entity expires at block ${e.expiresAt}`,
        `  $creator == issuer: ${byIssuer ? "yes" : "NO (Mandate ignores this entity)"}`,
        "",
      ].join("\n"),
    )
  }
  console.log(`${Math.min(count, MAX_RESULTS)} ${entityType} entities\n`)
}
console.log(
  mismatched === 0
    ? "Every entity was created by the issuer named in its signed payload."
    : `${mismatched} entities were not created by their payload's issuer; Mandate ignores them.`,
)
