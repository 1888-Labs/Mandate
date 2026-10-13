// Every read and write Mandate makes against Arkiv.
//
// Signed documents are stored as the exact bytes the Rust engine sent and
// returned as the exact bytes stored. They are never re-serialized: JSON
// numbers above 2^53 (e.g. nonces) would silently lose precision in
// JavaScript, which would break the mandate's ID and signature.

import { setDefaultAutoSelectFamilyAttemptTimeout } from "node:net"

import { createPublicClient, createWalletClient } from "@arkiv-network/sdk"
import { addr, bytes32, str, u64 } from "@arkiv-network/sdk/attr"
import { tiramisu } from "@arkiv-network/sdk/chains"
import { predictEntityKey, randomSalt } from "@arkiv-network/sdk/entity"
import { and, eq, type Expression, gt, or } from "@arkiv-network/sdk/query"
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

export type StoredDocument = {
  entityKey: Hex
  owner: Hex
  /** `$creator`: the wallet that created the entity. Immutable. */
  creator: Hex
  raw: string
  depth: number | null
}

export type Config = {
  projectId: string
  rpcUrl?: string | undefined
  /** Arkiv access key (hub.arkiv.network/api-keys), sent as `X-API-KEY`. */
  apiKey?: string | undefined
  /** Funder wallet: tops up issuer wallets with gas. Never an entity creator. */
  privateKey?: Hex | undefined
  /**
   * Keys of the principals this adapter may publish as. A mandate or
   * revocation is created by its *issuer's* wallet, so `$creator` equals
   * the issuer.
   */
  issuerKeys?: Hex[] | undefined
}

/** A lookup matched more entities than we read: refuse rather than truncate. */
export class TooManyResults extends Error {}

/** The adapter does not hold the key of the issuer it was asked to publish as. */
export class IssuerKeyUnavailable extends Error {}

/** Top up an issuer wallet when it falls below this (in wei)… */
const MIN_ISSUER_BALANCE = 2_000_000_000_000_000n // 0.002 GLM ≈ 13 writes
/** …with this much. */
const ISSUER_TOP_UP = 10_000_000_000_000_000n // 0.01 GLM ≈ 65 writes

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
  const funderAccount = config.privateKey ? privateKeyToAccount(config.privateKey) : undefined
  const funder = funderAccount
    ? createWalletClient({ chain: tiramisu, transport, account: funderAccount })
    : undefined
  const issuers = new Map(
    (config.issuerKeys ?? []).map((key) => {
      const account = privateKeyToAccount(key)
      const client = createWalletClient({ chain: tiramisu, transport, account })
      return [account.address.toLowerCase(), { account, client }] as const
    }),
  )
  const ns = [eq("project_id", str(config.projectId))]

  /** The wallet of `issuer`, funded with enough gas for a write. */
  async function issuerWallet(issuer: Hex) {
    const wallet = issuers.get(issuer.toLowerCase())
    if (!wallet) {
      throw new IssuerKeyUnavailable(
        `this adapter cannot publish as ${issuer}: entities are created by their issuer's own wallet`,
      )
    }
    const balance = await publicClient.getBalance({ address: wallet.account.address })
    if (balance < MIN_ISSUER_BALANCE) {
      if (!funder) throw new Error(`issuer ${issuer} has no gas and no funder is configured`)
      const hash = await funder.sendTransaction({
        to: wallet.account.address,
        value: ISSUER_TOP_UP,
      })
      await publicClient.waitForTransactionReceipt({ hash })
    }
    return wallet
  }

  /**
   * Every entity matching `filters` within our namespace, as raw JSON
   * documents with their `$creator`. Attributes are fetched only when
   * `withDepth` is set: every selected field counts against the RPC's query
   * budget. Fails closed with `TooManyResults` rather than silently
   * dropping matches past the cap.
   */
  async function find(
    entityType: string,
    filters: Expression[],
    withDepth = false,
  ): Promise<StoredDocument[]> {
    const where = and(...ns, eq("entity_type", str(entityType)), ...filters)
    const out: StoredDocument[] = []
    let seen = 0
    const keep = (key: Hex, owner: Hex, creator: Hex, text: () => string, depth: number | null) => {
      if (++seen > MAX_RESULTS) {
        throw new TooManyResults(`more than ${MAX_RESULTS} ${entityType} entities match`)
      }
      try {
        const raw = text()
        JSON.parse(raw) // skip junk others wrote into our namespace; keep the raw text
        out.push({ entityKey: key, owner, creator, raw, depth })
      } catch {}
    }
    if (withDepth) {
      const query = publicClient
        .select({ key: true, owner: true, creator: true, payload: true, attributes: true })
        .where(where)
        .limit(200)
      for await (const e of query) {
        const d = e.attributes.depth
        keep(e.key, e.owner, e.creator, () => e.toText(), d?.type === "i32" ? d.value : null)
      }
    } else {
      const query = publicClient
        .select({ key: true, owner: true, creator: true, payload: true })
        .where(where)
        .limit(200)
      for await (const e of query) {
        keep(e.key, e.owner, e.creator, () => e.toText(), null)
      }
    }
    return out
  }

  /**
   * Keep only entities whose `$creator` is the document's own issuer.
   * Anyone can write an entity claiming any issuer; only the issuer's
   * wallet can be its creator.
   */
  function issuerAuthored(docs: StoredDocument[]): StoredDocument[] {
    return docs.filter((d) => {
      const issuer: unknown = JSON.parse(d.raw).issuer
      return typeof issuer === "string" && issuer.toLowerCase() === d.creator.toLowerCase()
    })
  }

  /** `name = v1 OR name = v2 …` for up to MAX_IDS_PER_QUERY bytes32 values. */
  function anyOf(name: string, ids: Hex[]): Expression {
    checkBatch(ids.length)
    return or(...ids.map((id) => eq(name, bytes32(id))))
  }

  function checkBatch(n: number) {
    if (n === 0 || n > MAX_IDS_PER_QUERY) {
      throw new RangeError(`between 1 and ${MAX_IDS_PER_QUERY} ids per lookup`)
    }
  }

  return {
    funder: funderAccount?.address,
    issuers: [...issuers.values()].map((w) => w.account.address),

    async chainId() {
      return publicClient.getChainId()
    },

    async mandatesById(id: Hex, withDepth = false) {
      return issuerAuthored(await find("mandate", [eq("mandate_id", bytes32(id))], withDepth))
    },

    /** Every issuer-published copy of any of `ids`, in one query. */
    async mandatesByIds(ids: Hex[]) {
      return issuerAuthored(await find("mandate", [anyOf("mandate_id", ids)]))
    },

    /**
     * An agent's mandates. With `activeAt`, only those whose signed expiry
     * is later (a range query on the numeric `expires_at` attribute).
     */
    async mandatesForSubject(
      subject: Hex,
      action?: string,
      resource?: Resource,
      activeAt?: number,
    ) {
      const filters: Expression[] = [eq("subject", addr(subject))]
      if (action !== undefined) filters.push(eq("action", str(action)))
      if (resource !== undefined) {
        filters.push(eq("resource_hash", bytes32(resourceHash(resource))))
      }
      if (activeAt !== undefined) filters.push(gt("expires_at", u64(BigInt(activeAt))))
      return issuerAuthored(await find("mandate", filters))
    },

    async children(parentId: Hex) {
      return issuerAuthored(await find("mandate", [eq("parent_id", bytes32(parentId))]))
    },

    /**
     * Revocations of each mandate *created by that mandate's issuer*:
     * `(mandate_id = A AND $creator = issuerA) OR …`. Filtering on
     * `$creator` inside the query means nobody else's entities can crowd a
     * real revocation out of the results.
     */
    async revocationsFor(targets: { mandateId: Hex; issuer: Hex }[]) {
      checkBatch(targets.length)
      const docs = await find("revocation", [
        or(
          ...targets.map((t) =>
            and(eq("mandate_id", bytes32(t.mandateId)), eq("$creator", addr(t.issuer))),
          ),
        ),
      ])
      return issuerAuthored(docs)
    },

    /**
     * Store a signed mandate, created by its issuer's wallet. The entity
     * expires with the mandate and is read-only. Re-publishing the exact same
     * document is a no-op.
     */
    async createMandate(raw: string, m: Mandate) {
      const { client } = await issuerWallet(m.issuer)
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

      const { entityKey, txHash } = await client.createEntity({
        payload: stringToBytes(raw),
        contentType: "application/json",
        attributes: mandateAttributes(config.projectId, m, depth),
        expires: ExpirationTime.atDate(new Date(m.expiresAt * 1000)),
        flags: { readonly: true },
      })
      return { entityKey, txHash, created: true }
    },

    /**
     * Store a signed revocation, created by the revoker's wallet and kept at
     * least until `retainUntil` (the revoked mandate's expiry). In the same
     * transaction it is handed to the burn address, so not even the revoker
     * can delete it later.
     */
    async createRevocation(raw: string, r: Revocation, retainUntil: number) {
      const { client, account } = await issuerWallet(r.issuer)
      const existing = await this.revocationsFor([{ mandateId: r.mandateId, issuer: r.issuer }])
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
      const { txHash, createdEntities } = await client.executeBatch({
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
