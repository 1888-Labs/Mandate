// HTTP bridge between the Mandate Rust engine and Arkiv.
//
//   GET  /health
//   GET  /v1/mandates/:id                         every stored copy of a mandate ID
//   GET  /v1/mandates?id=&id=…                    every copy of several IDs, in one query
//   GET  /v1/mandates?subject=&action=&resource=  candidates for a subject (resource as kind:id)
//        …&active=1                               only those not yet expired (range query)
//   GET  /v1/mandates?parent=                     children of a mandate
//   GET  /v1/revocations?of=<id>:<issuer>&of=…    revocations of each mandate created by its issuer
//   POST /v1/mandates                             body: signed mandate JSON
//   POST /v1/revocations?retain_until=<unix>      body: signed revocation JSON
//   GET  /v1/events                               live entity events (Server-Sent Events)
//
// Every document returned was created ($creator) by its own issuer's wallet;
// copies created by anyone else are dropped. Writes are made from the
// issuer's wallet (ARKIV_ISSUER_KEYS) and require
// `Authorization: Bearer $ADAPTER_TOKEN` when ADAPTER_TOKEN is set.

import { createServer, type IncomingMessage, type ServerResponse } from "node:http"
import type { Hex } from "viem"

import {
  type Arkiv,
  connect,
  IssuerKeyUnavailable,
  MAX_IDS_PER_QUERY,
  rateLimitOf,
  type StoredDocument,
  TooManyResults,
} from "./arkiv.ts"
import { InvalidDocument, parseMandate, parseRevocation, type Resource } from "./documents.ts"
import type { liveEvents } from "./events.ts"

const MAX_BODY_BYTES = 128 * 1024
const HASH = /^0x[0-9a-f]{64}$/
const ADDRESS = /^0x[0-9a-f]{40}$/

class HttpError extends Error {
  readonly status: number
  readonly code: string
  constructor(status: number, code: string, message: string) {
    super(message)
    this.status = status
    this.code = code
  }
}

function send(res: ServerResponse, status: number, body: string) {
  res.writeHead(status, { "content-type": "application/json" })
  res.end(body)
}

function sendJson(res: ServerResponse, status: number, value: unknown) {
  send(res, status, JSON.stringify(value))
}

/** Documents are spliced in as stored text, never re-serialized. */
function sendDocuments(res: ServerResponse, docs: StoredDocument[]) {
  const items = docs.map(
    (d) =>
      `{"entityKey":${JSON.stringify(d.entityKey)},"creator":${JSON.stringify(d.creator)},"owner":${JSON.stringify(d.owner)},"document":${d.raw}}`,
  )
  send(res, 200, `{"documents":[${items.join(",")}]}`)
}

async function readBody(req: IncomingMessage): Promise<string> {
  const chunks: Buffer[] = []
  let size = 0
  for await (const chunk of req) {
    size += (chunk as Buffer).length
    if (size > MAX_BODY_BYTES) throw new HttpError(413, "BODY_TOO_LARGE", "body over 128 KiB")
    chunks.push(chunk as Buffer)
  }
  return Buffer.concat(chunks).toString("utf8")
}

function parseJson(raw: string): unknown {
  try {
    return JSON.parse(raw)
  } catch {
    throw new HttpError(400, "INVALID_JSON", "body is not valid JSON")
  }
}

function hashParam(value: string | null, name: string): Hex {
  if (value === null || !HASH.test(value)) {
    throw new HttpError(400, "INVALID_PARAMETER", `${name} must be 0x + 64 lowercase hex`)
  }
  return value as Hex
}

function hashParams(values: string[], name: string): Hex[] {
  if (values.length === 0 || values.length > MAX_IDS_PER_QUERY) {
    throw new HttpError(
      400,
      "INVALID_PARAMETER",
      `between 1 and ${MAX_IDS_PER_QUERY} ${name} values per request`,
    )
  }
  return values.map((v) => hashParam(v, name))
}

/** `of=<mandateId>:<issuer>` pairs. */
function targetParams(values: string[]): { mandateId: Hex; issuer: Hex }[] {
  if (values.length === 0 || values.length > MAX_IDS_PER_QUERY) {
    throw new HttpError(
      400,
      "INVALID_PARAMETER",
      `between 1 and ${MAX_IDS_PER_QUERY} of= values per request`,
    )
  }
  return values.map((v) => {
    const [mandateId, issuer] = v.split(":")
    if (!mandateId || !HASH.test(mandateId) || !issuer || !ADDRESS.test(issuer)) {
      throw new HttpError(400, "INVALID_PARAMETER", "of= must be <mandate id>:<issuer address>")
    }
    return { mandateId: mandateId as Hex, issuer: issuer as Hex }
  })
}

function resourceParam(value: string | null): Resource | undefined {
  if (value === null) return undefined
  const i = value.indexOf(":")
  if (i <= 0 || i === value.length - 1) {
    throw new HttpError(400, "INVALID_PARAMETER", "resource must be kind:id")
  }
  return { kind: value.slice(0, i), id: value.slice(i + 1) }
}

function requireToken(req: IncomingMessage, token: string | undefined) {
  if (token && req.headers.authorization !== `Bearer ${token}`) {
    throw new HttpError(401, "UNAUTHORIZED", "writes require the adapter token")
  }
}

export function handler(
  arkiv: Arkiv,
  writeToken: string | undefined,
  events?: ReturnType<typeof liveEvents>,
) {
  return async (req: IncomingMessage, res: ServerResponse) => {
    const url = new URL(req.url ?? "/", "http://adapter")
    const path = url.pathname.replace(/\/+$/, "")
    const q = url.searchParams
    try {
      if (req.method === "GET" && path === "/health") {
        return sendJson(res, 200, {
          ok: true,
          chainId: await arkiv.chainId(),
          funder: arkiv.funder ?? null,
          issuers: arkiv.issuers,
        })
      }

      if (req.method === "GET" && path === "/v1/events") {
        if (!events) throw new HttpError(404, "NOT_FOUND", "live events are not enabled")
        return events.subscribe(res)
      }

      const byId = path.match(/^\/v1\/mandates\/([^/]+)$/)
      if (req.method === "GET" && byId) {
        return sendDocuments(res, await arkiv.mandatesById(hashParam(byId[1] ?? null, "id")))
      }

      if (req.method === "GET" && path === "/v1/mandates") {
        if (q.has("id")) {
          return sendDocuments(res, await arkiv.mandatesByIds(hashParams(q.getAll("id"), "id")))
        }
        if (q.has("parent")) {
          return sendDocuments(res, await arkiv.children(hashParam(q.get("parent"), "parent")))
        }
        const subject = q.get("subject")
        if (subject === null || !ADDRESS.test(subject)) {
          throw new HttpError(400, "INVALID_PARAMETER", "subject must be a lowercase address")
        }
        const action = q.get("action") ?? undefined
        const resource = resourceParam(q.get("resource"))
        const activeAt = q.has("active") ? Math.floor(Date.now() / 1000) : undefined
        return sendDocuments(
          res,
          await arkiv.mandatesForSubject(subject as Hex, action, resource, activeAt),
        )
      }

      if (req.method === "GET" && path === "/v1/revocations") {
        return sendDocuments(
          res,
          await arkiv.revocationsFor(targetParams(q.getAll("of"))),
        )
      }

      if (req.method === "POST" && path === "/v1/mandates") {
        requireToken(req, writeToken)
        const raw = await readBody(req)
        const mandate = parseMandate(parseJson(raw))
        const result = await arkiv.createMandate(raw, mandate)
        console.log(`mandate ${mandate.id} stored: entity ${result.entityKey}`)
        return sendJson(res, result.created ? 201 : 200, result)
      }

      if (req.method === "POST" && path === "/v1/revocations") {
        requireToken(req, writeToken)
        const retainUntil = Number(q.get("retain_until"))
        if (!Number.isSafeInteger(retainUntil) || retainUntil <= 0) {
          throw new HttpError(400, "INVALID_PARAMETER", "retain_until must be a Unix time")
        }
        const raw = await readBody(req)
        const revocation = parseRevocation(parseJson(raw))
        const result = await arkiv.createRevocation(raw, revocation, retainUntil)
        console.log(`revocation ${revocation.id} stored: entity ${result.entityKey}`)
        return sendJson(res, result.created ? 201 : 200, result)
      }

      throw new HttpError(404, "NOT_FOUND", `${req.method} ${path} not found`)
    } catch (e) {
      if (e instanceof HttpError) {
        return sendJson(res, e.status, { error: e.code, message: e.message })
      }
      if (e instanceof InvalidDocument) {
        return sendJson(res, 400, { error: "INVALID_DOCUMENT", message: e.message })
      }
      if (e instanceof IssuerKeyUnavailable) {
        return sendJson(res, 403, { error: "ISSUER_KEY_UNAVAILABLE", message: e.message })
      }
      if (e instanceof TooManyResults) {
        // Fail closed: a truncated answer could hide a revocation.
        console.error(`${req.method} ${path}: ${e.message}`)
        return sendJson(res, 503, { error: "TOO_MANY_RESULTS", message: e.message })
      }
      const limited = rateLimitOf(e)
      if (limited) {
        console.error(`${req.method} ${path}: Arkiv rate limit (retry after ${limited.retryAfter}s)`)
        return sendJson(res, 503, {
          error: "ARKIV_RATE_LIMITED",
          message: `${limited.message}. Set ARKIV_API_KEY (https://hub.arkiv.network/api-keys).`,
          retryAfter: limited.retryAfter,
        })
      }
      const message = e instanceof Error ? e.message : String(e)
      console.error(`${req.method} ${path} failed: ${message}`)
      return sendJson(res, 502, { error: "ARKIV_ERROR", message })
    }
  }
}

if (import.meta.main) {
  const projectId = process.env.ARKIV_PROJECT_ID ?? "mandate-global-tour-2026"
  const arkiv = connect({
    projectId,
    rpcUrl: process.env.ARKIV_RPC_URL || undefined,
    apiKey: process.env.ARKIV_API_KEY || undefined,
    privateKey: (process.env.ARKIV_PRIVATE_KEY as Hex | undefined) || undefined,
    issuerKeys: (process.env.ARKIV_ISSUER_KEYS ?? "")
      .split(",")
      .map((k) => k.trim())
      .filter((k) => k.length > 0) as Hex[],
  })
  const port = Number(process.env.PORT ?? 8787)
  const host = process.env.HOST ?? "127.0.0.1"
  const { liveEvents } = await import("./events.ts")
  const events = liveEvents({
    projectId,
    watched: arkiv.issuers,
    apiKey: process.env.ARKIV_API_KEY || undefined,
    wsUrl: process.env.ARKIV_WS_URL || undefined,
  })
  const server = createServer(handler(arkiv, process.env.ADAPTER_TOKEN || undefined, events))
  server.listen(port, host, () => {
    console.log(`arkiv-adapter on http://${host}:${port}  project=${projectId}`)
    console.log(`funder ${arkiv.funder ?? "(none)"}`)
    console.log(
      arkiv.issuers.length > 0
        ? `publishes as ${arkiv.issuers.join(", ")}`
        : "read-only: no ARKIV_ISSUER_KEYS, so it cannot publish",
    )
    if (!process.env.ARKIV_API_KEY) {
      console.log("no ARKIV_API_KEY: using the anonymous RPC budget, which is easy to exhaust")
    }
  })
}
