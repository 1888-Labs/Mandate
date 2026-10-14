// Live Arkiv activity for the web UI.
//
// One `watchEntityEvents` subscription over a WebSocket transport, with no
// `fromBlock`: the node pushes logs as blocks land, instead of being polled.
// Events cover the whole chain, so only those from wallets we publish as
// (our issuers) are kept, and each is checked against our namespace before it
// is relayed to browsers as Server-Sent Events.

import type { ServerResponse } from "node:http"

import { createPublicClient } from "@arkiv-network/sdk"
import { tiramisu } from "@arkiv-network/sdk/chains"
import type { EntityEvent } from "@arkiv-network/sdk/types"
import { type Hex, webSocket } from "viem"

export type LiveEvent = {
  kind: "mandate" | "revocation"
  action: "created" | "deleted"
  entityKey: Hex
  creator: Hex
  /** The mandate it is (or revokes). */
  mandateId: string | null
  txHash: Hex
  blockNumber: string
}

const WS_URL = "wss://rpc.tiramisu.db-chain.testnet.arkiv.network"

export function liveEvents(options: {
  projectId: string
  /** Wallets whose entities we relay: the issuers this adapter publishes as. */
  watched: Hex[]
  apiKey?: string | undefined
  wsUrl?: string | undefined
}) {
  const watched = new Set(options.watched.map((a) => a.toLowerCase()))
  // The RPC accepts an access key in the WebSocket path.
  const base = (options.wsUrl ?? WS_URL).replace(/\/$/, "")
  const url = options.apiKey ? `${base}/${options.apiKey}` : base
  const client = createPublicClient({ chain: tiramisu, transport: webSocket(url) })
  const clients = new Set<ServerResponse>()
  let unwatch: (() => void) | undefined
  let heartbeat: ReturnType<typeof setInterval> | undefined

  function broadcast(event: LiveEvent) {
    const frame = `event: arkiv\ndata: ${JSON.stringify(event)}\n\n`
    for (const res of clients) res.write(frame)
  }

  async function relay(e: EntityEvent) {
    let action: LiveEvent["action"]
    let owner: Hex
    if (e.type === "EntityCreated") [action, owner] = ["created", e.owner]
    else if (e.type === "EntityDeleted") [action, owner] = ["deleted", e.owner]
    else return // patches, extensions and transfers carry nothing new for the UI
    if (!watched.has(owner.toLowerCase())) return
    try {
      const entity = await client.getEntity(e.entityKey)
      const attr = entity.attributes ?? {}
      if (attr.project_id?.value !== options.projectId) return
      const kind = attr.entity_type?.value
      if (kind !== "mandate" && kind !== "revocation") return
      broadcast({
        kind,
        action,
        entityKey: e.entityKey,
        creator: entity.creator ?? owner,
        mandateId: typeof attr.mandate_id?.value === "string" ? attr.mandate_id.value : null,
        txHash: e.transactionHash,
        blockNumber: e.blockNumber.toString(),
      })
    } catch {
      // A deleted entity can't be read back; relay what the event itself says.
      if (action === "deleted") {
        broadcast({
          kind: "mandate",
          action,
          entityKey: e.entityKey,
          creator: owner,
          mandateId: null,
          txHash: e.transactionHash,
          blockNumber: e.blockNumber.toString(),
        })
      }
    }
  }

  function start() {
    if (unwatch) return
    unwatch = client.watchEntityEvents({
      onEvent: (e) => void relay(e),
      onError: (error) => console.error(`entity events: ${error.message}`),
    })
    heartbeat = setInterval(() => {
      for (const res of clients) res.write(": keep-alive\n\n")
    }, 15_000)
  }

  function stop() {
    unwatch?.()
    unwatch = undefined
    if (heartbeat) clearInterval(heartbeat)
    heartbeat = undefined
  }

  return {
    /** Stream events to one HTTP client until it disconnects. */
    subscribe(res: ServerResponse) {
      res.writeHead(200, {
        "content-type": "text/event-stream",
        "cache-control": "no-cache",
        connection: "keep-alive",
      })
      res.write(`event: ready\ndata: ${JSON.stringify({ watching: [...watched] })}\n\n`)
      clients.add(res)
      start()
      res.on("close", () => {
        clients.delete(res)
        if (clients.size === 0) stop()
      })
    },
  }
}
