// HTTP bridge between the Mandate Rust engine and Arkiv.
//
//   GET  /health
//   GET  /v1/mandates/:id                         every stored copy of a mandate ID
//   GET  /v1/mandates?subject=&action=&resource=  candidates for a subject (resource as kind:id)
//   GET  /v1/mandates?parent=                     children of a mandate
//   GET  /v1/revocations?mandate=                 every revocation claiming a mandate
//   POST /v1/mandates                             body: signed mandate JSON
//   POST /v1/revocations?retain_until=<unix>      body: signed revocation JSON
//
// Writes require `Authorization: Bearer $ADAPTER_TOKEN` when ADAPTER_TOKEN is
// set, because they spend the adapter wallet's gas.

import { createServer, type IncomingMessage, type ServerResponse } from "node:http"
import type { Hex } from "viem"

import { type Arkiv, connect, type StoredDocument } from "./arkiv.ts"
import { InvalidDocument, parseMandate, parseRevocation, type Resource } from "./documents.ts"

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
      `{"entityKey":${JSON.stringify(d.entityKey)},"owner":${JSON.stringify(d.owner)},"document":${d.raw}}`,
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

export function handler(arkiv: Arkiv, writeToken: string | undefined) {
  return async (req: IncomingMessage, res: ServerResponse) => {
    const url = new URL(req.url ?? "/", "http://adapter")
    const path = url.pathname.replace(/\/+$/, "")
    const q = url.searchParams
    try {
      if (req.method === "GET" && path === "/health") {
        return sendJson(res, 200, {
          ok: true,
          chainId: await arkiv.chainId(),
          writer: arkiv.writer ?? null,
        })
      }

      const byId = path.match(/^\/v1\/mandates\/([^/]+)$/)
      if (req.method === "GET" && byId) {
        return sendDocuments(res, await arkiv.mandatesById(hashParam(byId[1] ?? null, "id")))
      }

      if (req.method === "GET" && path === "/v1/mandates") {
        if (q.has("parent")) {
          return sendDocuments(res, await arkiv.children(hashParam(q.get("parent"), "parent")))
        }
        const subject = q.get("subject")
        if (subject === null || !ADDRESS.test(subject)) {
          throw new HttpError(400, "INVALID_PARAMETER", "subject must be a lowercase address")
        }
        const action = q.get("action") ?? undefined
        const resource = resourceParam(q.get("resource"))
        return sendDocuments(
          res,
          await arkiv.mandatesForSubject(subject as Hex, action, resource),
        )
      }

      if (req.method === "GET" && path === "/v1/revocations") {
        return sendDocuments(res, await arkiv.revocationsFor(hashParam(q.get("mandate"), "mandate")))
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
    privateKey: (process.env.ARKIV_PRIVATE_KEY as Hex | undefined) || undefined,
  })
  const port = Number(process.env.PORT ?? 8787)
  const host = process.env.HOST ?? "127.0.0.1"
  createServer(handler(arkiv, process.env.ADAPTER_TOKEN || undefined)).listen(port, host, () => {
    console.log(`arkiv-adapter on http://${host}:${port}  project=${projectId}`)
    console.log(arkiv.writer ? `writer ${arkiv.writer}` : "read-only (no ARKIV_PRIVATE_KEY)")
  })
}
