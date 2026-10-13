// Read-only connectivity check: chain ID, head block, and a namespaced query.
import { createPublicClient } from "@arkiv-network/sdk"
import { tiramisu } from "@arkiv-network/sdk/chains"
import { and, eq } from "@arkiv-network/sdk/query"
import { http } from "viem"

const client = createPublicClient({ chain: tiramisu, transport: http(process.env.ARKIV_RPC_URL) })
console.log("chain id:", await client.getChainId())
console.log("head block:", await client.getBlockNumber())
const result = await client
  .select({ key: true })
  .where(and(eq("project_id", "mandate-global-tour-2026"), eq("entity_type", "mandate")))
  .limit(5)
  .fetch()
console.log("mandate entities in our namespace:", result.entities.length)
