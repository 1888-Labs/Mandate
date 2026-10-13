// Every read and write Mandate makes against Arkiv.
//
// Signed documents are stored as the exact bytes the Rust engine sent and
// returned as the exact bytes stored. They are never re-serialized: JSON
// numbers above 2^53 (e.g. nonces) would silently lose precision in
// JavaScript, which would break the mandate's ID and signature.

import { setDefaultAutoSelectFamilyAttemptTimeout } from "node:net"

import { createPublicClient, createWalletClient } from "@arkiv-network/sdk"
import { addr, bytes32, str } from "@arkiv-network/sdk/attr"
import { tiramisu } from "@arkiv-network/sdk/chains"
import { predictEntityKey, randomSalt } from "@arkiv-network/sdk/entity"
import { and, eq, type Expression, or } from "@arkiv-network/sdk/query"
import { ExpirationTime } from "@arkiv-network/sdk/utils"
import { type Hex, http, stringToBytes } from "viem"
import { privateKeyToAccount } from "viem/accounts"

import {
  type Mandate,
  mandateAttributes,
  type Resource,
  type Revocation,
  resourceHash,
  revocationAttributes,
} from "./documents.ts"

/**
 * Revocations are handed to this address right after creation, in the same
 * transaction. Nobody holds its key, so nobody (including the adapter's
 * own wallet) can delete a revocation and bring a revoked mandate back.
 */
export const BURN_ADDRESS: Hex = "0x000000000000000000000000000000000000dEaD"

/** Keep revocations this long past the mandate they revoke, for clock skew. */
const REVOCATION_MARGIN_SECONDS = 3600

/** Upper bound on entities returned for one lookup, against spam in our namespace. */
const MAX_RESULTS = 1000

/** Most IDs combined into one OR query. */
export const MAX_IDS_PER_QUERY = 50

/**
 * The Arkiv RPC refused a call because a query budget is spent (HTTP 429).
 * Raised immediately instead of letting viem sleep for `Retry-After`
 * (observed: 609 s), which looked like a hang.
 */
export class ArkivRateLimited extends Error {
  readonly retryAfter: number | null
  constructor(message: string, retryAfter: number | null) {
    super(message)
    this.retryAfter = retryAfter
  }
}

/** Find an `ArkivRateLimited` anywhere in an error's cause chain. */
export function rateLimitOf(error: unknown): ArkivRateLimited | undefined {
  for (let e = error; e instanceof Error; e = e.cause) {
    if (e instanceof ArkivRateLimited) return e
  }
  return undefined
}

const failFastOn429: typeof fetch = async (input, init) => {
  const res = await fetch(input, init)
  if (res.status !== 429) return res
  const retryAfter = Number(res.headers.get("retry-after"))
  let message = "Arkiv RPC query budget exhausted"
  try {
    message = (await res.json()).message ?? message
  } catch {}
  throw new ArkivRateLimited(message, Number.isFinite(retryAfter) ? retryAfter : null)
}

export type StoredDocument = { entityKey: Hex; owner: Hex; raw: string; depth: number | null }

export type Config = {
  projectId: string
  rpcUrl?: string | undefined
  /** Arkiv access key (hub.arkiv.network/api-keys), sent as `X-API-KEY`. */
  apiKey?: string | undefined
  privateKey?: Hex | undefined
}

// Node gives each resolved address only 250 ms to connect before giving up
// on it. On a slow link that fails most RPC calls (measured: 16 of 25 with
// ETIMEDOUT); 2 s fixed every one. See arkiv/friction.md.
setDefaultAutoSelectFamilyAttemptTimeout(2_000)

export function connect(config: Config) {
  const transport = http(config.rpcUrl, {
    retryCount: 2,
    timeout: 30_000,
    fetchFn: failFastOn429,
    fetchOptions: config.apiKey ? { headers: { "X-API-KEY": config.apiKey } } : {},
  })
  const publicClient = createPublicClient({ chain: tiramisu, transport })
  const account = config.privateKey ? privateKeyToAccount(config.privateKey) : undefined
  const walletClient = account
    ? createWalletClient({ chain: tiramisu, transport, account })
    : undefined
  const ns = [eq("project_id", str(config.projectId))]

  function wallet() {
    if (!walletClient || !account) throw new Error("no ARKIV_PRIVATE_KEY: adapter is read-only")
    return { walletClient, account }
  }

  /**
   * Every entity matching `filters` within our namespace, as raw JSON
   * documents. Attributes are fetched only when `withDepth` is set: every
   * selected field counts against the RPC's query budget.
   */
  async function find(
    entityType: string,
    filters: Expression[],
    withDepth = false,
  ): Promise<StoredDocument[]> {
    const where = and(...ns, eq("entity_type", str(entityType)), ...filters)
    const out: StoredDocument[] = []
    const keep = (key: Hex, owner: Hex, text: () => string, depth: number | null) => {
      try {
        const raw = text()
        JSON.parse(raw) // skip junk others wrote into our namespace; keep the raw text
        out.push({ entityKey: key, owner, raw, depth })
      } catch {}
    }
    if (withDepth) {
      const query = publicClient
        .select({ key: true, owner: true, payload: true, attributes: true })
        .where(where)
        .limit(200)
      for await (const e of query) {
        if (out.length >= MAX_RESULTS) break
        const d = e.attributes.depth
        keep(e.key, e.owner, () => e.toText(), d?.type === "i32" ? d.value : null)
      }
    } else {
      const query = publicClient.select({ key: true, owner: true, payload: true }).where(where).limit(200)
      for await (const e of query) {
        if (out.length >= MAX_RESULTS) break
        keep(e.key, e.owner, () => e.toText(), null)
      }
    }
    return out
  }

  /** `name = v1 OR name = v2 …` for up to MAX_IDS_PER_QUERY bytes32 values. */
  function anyOf(name: string, ids: Hex[]): Expression {
    if (ids.length === 0 || ids.length > MAX_IDS_PER_QUERY) {
      throw new RangeError(`between 1 and ${MAX_IDS_PER_QUERY} ids per lookup`)
    }
    return or(...ids.map((id) => eq(name, bytes32(id))))
  }

  return {
    writer: account?.address,

    async chainId() {
      return publicClient.getChainId()
    },

    mandatesById(id: Hex, withDepth = false) {
      return find("mandate", [eq("mandate_id", bytes32(id))], withDepth)
    },

    /** Every copy of any of `ids`, in one query. */
    mandatesByIds(ids: Hex[]) {
      return find("mandate", [anyOf("mandate_id", ids)])
    },

    mandatesForSubject(subject: Hex, action?: string, resource?: Resource) {
      const filters: Expression[] = [eq("subject", addr(subject))]
      if (action !== undefined) filters.push(eq("action", str(action)))
      if (resource !== undefined) {
        filters.push(eq("resource_hash", bytes32(resourceHash(resource))))
      }
      return find("mandate", filters)
    },

    children(parentId: Hex) {
      return find("mandate", [eq("parent_id", bytes32(parentId))])
    },

    revocationsFor(mandateId: Hex) {
      return find("revocation", [eq("mandate_id", bytes32(mandateId))])
    },

    /** Every revocation of any of `mandateIds`, in one query. */
    revocationsForMany(mandateIds: Hex[]) {
      return find("revocation", [anyOf("mandate_id", mandateIds)])
    },

    /**
     * Store a signed mandate. The entity expires with the mandate and is
     * read-only. Storing the exact same document twice is a no-op.
     */
    async createMandate(raw: string, m: Mandate) {
      const { walletClient } = wallet()
      const existing = await this.mandatesById(m.id)
      const same = existing.find((d) => JSON.parse(d.raw).signature === m.signature)
      if (same) return { entityKey: same.entityKey, txHash: null, created: false }

      const now = Math.floor(Date.now() / 1000)
      if (m.expiresAt <= now) throw new Error("mandate has already expired")

      let depth: number | null = 0
      if (m.parent) {
        // Informational only: storage is untrusted, the engine walks the real chain.
        const parents = await this.mandatesById(m.parent, true)
        const parentDepth = parents.map((p) => p.depth).find((d) => d !== null)
        depth = parentDepth === undefined || parentDepth === null ? null : parentDepth + 1
      }

      const { entityKey, txHash } = await walletClient.createEntity({
        payload: stringToBytes(raw),
        contentType: "application/json",
        attributes: mandateAttributes(config.projectId, m, depth),
        expires: ExpirationTime.atDate(new Date(m.expiresAt * 1000)),
        flags: { readonly: true },
      })
      return { entityKey, txHash, created: true }
    },

    /**
     * Store a signed revocation, kept at least until `retainUntil` (the
     * revoked mandate's expiry), then hand it to the burn address in the
     * same transaction so it can never be deleted.
     */
    async createRevocation(raw: string, r: Revocation, retainUntil: number) {
      const { walletClient, account } = wallet()
      const existing = await this.revocationsFor(r.mandateId)
      const same = existing.find((d) => JSON.parse(d.raw).id === r.id)
      if (same) return { entityKey: same.entityKey, txHash: null, created: false }

      const now = Math.floor(Date.now() / 1000)
      const keepUntil = Math.max(retainUntil, now) + REVOCATION_MARGIN_SECONDS
      const salt = randomSalt()
      const nonce = await publicClient.getEntityNonce(account.address)
      const entityKey = predictEntityKey({
        owner: account.address,
        nonce,
        salt,
        chainId: await publicClient.getChainId(),
      })
      const { txHash, createdEntities } = await walletClient.executeBatch({
        creates: [
          {
            payload: stringToBytes(raw),
            contentType: "application/json",
            attributes: revocationAttributes(config.projectId, r),
            expires: ExpirationTime.atDate(new Date(keepUntil * 1000)),
            flags: { readonly: true },
            salt,
          },
        ],
        ownershipChanges: [{ entityKey, newOwner: BURN_ADDRESS }],
      })
      if (createdEntities[0]?.toLowerCase() !== entityKey.toLowerCase()) {
        throw new Error(
          `predicted revocation key ${entityKey} but Arkiv created ${createdEntities[0]}`,
        )
      }
      return { entityKey, txHash, created: true }
    },
  }
}

export type Arkiv = ReturnType<typeof connect>
