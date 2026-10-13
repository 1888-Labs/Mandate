// Offline tests: document parsing, attribute derivation, and the HTTP
// layer against a fake Arkiv. No network access.

import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { createServer } from "node:http"
import type { AddressInfo } from "node:net"
import { test } from "node:test"

import type { Arkiv, StoredDocument } from "../src/arkiv.ts"
import {
  InvalidDocument,
  mandateAttributes,
  parseMandate,
  parseRevocation,
  resourceHash,
} from "../src/documents.ts"
import { handler } from "../src/server.ts"

const fixture = (name: string) =>
  readFileSync(new URL(`./fixtures/${name}`, import.meta.url), "utf8")

const MANDATE_RAW = fixture("mandate.json")
const REVOCATION_RAW = fixture("revocation.json")
/** The exact nonce digits in the fixture: larger than 2^53. */
const NONCE_DIGITS = MANDATE_RAW.match(/"nonce": (\d+)/)?.[1] ?? ""

test("fixture nonce is beyond JavaScript's exact integer range", () => {
  assert.ok(BigInt(NONCE_DIGITS) > BigInt(Number.MAX_SAFE_INTEGER))
  // Which is why a parse + stringify round trip would corrupt it:
  assert.notEqual(String(JSON.parse(MANDATE_RAW).nonce), NONCE_DIGITS)
})

test("parses a mandate produced by the Rust CLI", () => {
  const m = parseMandate(JSON.parse(MANDATE_RAW))
  assert.equal(m.action, "transfer")
  assert.deepEqual(m.resource, { kind: "wallet", id: "treasury-demo" })
  assert.equal(parseRevocation(JSON.parse(REVOCATION_RAW)).reason, "user_revoked")
})

test("rejects malformed mandates", () => {
  const good = JSON.parse(MANDATE_RAW)
  const bad = [
    null,
    { ...good, id: "0x1234" },
    { ...good, id: good.id.toUpperCase() },
    { ...good, issuer: "alice" },
    { ...good, subject: good.subject.replace("0x", "0X") },
    { ...good, resource: "wallet:treasury-demo" },
    { ...good, parent: "nope" },
    { ...good, delegable: "yes" },
    { ...good, expiresAt: -1 },
    { ...good, signature: "deadbeef" },
    { ...good, action: "x".repeat(129) },
  ]
  for (const b of bad) assert.throws(() => parseMandate(b), InvalidDocument, JSON.stringify(b))
})

test("resource hash is length-prefixed so kind/id boundaries can't shift", () => {
  assert.notEqual(resourceHash({ kind: "ab", id: "c" }), resourceHash({ kind: "a", id: "bc" }))
  assert.equal(
    resourceHash({ kind: "wallet", id: "treasury-demo" }),
    resourceHash({ kind: "wallet", id: "treasury-demo" }),
  )
})

test("mandate attributes are namespaced and typed", () => {
  const m = parseMandate(JSON.parse(MANDATE_RAW))
  const a = mandateAttributes("proj", m, 1)
  assert.equal(a.project_id.value, "proj")
  assert.equal(a.entity_type.value, "mandate")
  assert.equal(a.mandate_id.type, "bytes32")
  assert.equal(a.subject.type, "addr")
  assert.equal(a.parent_id?.type, "bytes32")
  assert.equal(a.depth?.value, 1)
  const root = mandateAttributes("proj", { ...m, parent: null }, null)
  assert.equal(root.parent_id, undefined)
  assert.equal(root.depth, undefined)
})

/** A fake Arkiv that records writes and serves them back. */
function fakeArkiv() {
  const stored: { raw: string }[] = []
  const doc = (raw: string): StoredDocument => ({
    entityKey: `0x${"ab".repeat(32)}`,
    owner: `0x${"cd".repeat(20)}`,
    raw,
    depth: null,
  })
  const fake = {
    writer: undefined,
    chainId: async () => 7738577,
    mandatesById: async () => stored.map((s) => doc(s.raw)),
    mandatesForSubject: async () => stored.map((s) => doc(s.raw)),
    children: async () => [],
    revocationsFor: async () => [],
    createMandate: async (raw: string) => {
      stored.push({ raw })
      return { entityKey: `0x${"ab".repeat(32)}`, txHash: null, created: true }
    },
    createRevocation: async () => {
      throw new Error("not used")
    },
  }
  return fake as unknown as Arkiv
}

async function withServer(
  token: string | undefined,
  fn: (base: string) => Promise<void>,
): Promise<void> {
  const server = createServer(handler(fakeArkiv(), token))
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve))
  const { port } = server.address() as AddressInfo
  try {
    await fn(`http://127.0.0.1:${port}`)
  } finally {
    server.close()
  }
}

test("stores and returns documents byte-for-byte (large nonce survives)", async () => {
  await withServer(undefined, async (base) => {
    const post = await fetch(`${base}/v1/mandates`, { method: "POST", body: MANDATE_RAW })
    assert.equal(post.status, 201)

    const id = JSON.parse(MANDATE_RAW).id
    const get = await fetch(`${base}/v1/mandates/${id}`)
    const text = await get.text()
    assert.ok(text.includes(`"nonce": ${NONCE_DIGITS}`), "nonce digits must be unchanged")
    assert.ok(text.includes(MANDATE_RAW), "document must be returned verbatim")
  })
})

test("writes require the token when one is configured", async () => {
  await withServer("s3cret", async (base) => {
    const denied = await fetch(`${base}/v1/mandates`, { method: "POST", body: MANDATE_RAW })
    assert.equal(denied.status, 401)
    const ok = await fetch(`${base}/v1/mandates`, {
      method: "POST",
      body: MANDATE_RAW,
      headers: { authorization: "Bearer s3cret" },
    })
    assert.equal(ok.status, 201)
  })
})

test("rejects bad input with stable error codes", async () => {
  await withServer(undefined, async (base) => {
    const cases: [string, RequestInit | undefined, number, string][] = [
      ["/v1/mandates", { method: "POST", body: "{not json" }, 400, "INVALID_JSON"],
      ["/v1/mandates", { method: "POST", body: "{}" }, 400, "INVALID_DOCUMENT"],
      ["/v1/mandates/0x12", undefined, 400, "INVALID_PARAMETER"],
      ["/v1/mandates?subject=alice", undefined, 400, "INVALID_PARAMETER"],
      [
        `/v1/mandates?subject=0x${"ab".repeat(20)}&resource=nocolon`,
        undefined,
        400,
        "INVALID_PARAMETER",
      ],
      ["/v1/revocations?retain_until=x", { method: "POST", body: "{}" }, 400, "INVALID_PARAMETER"],
      ["/v1/nope", undefined, 404, "NOT_FOUND"],
      ["/v1/mandates", { method: "POST", body: "x".repeat(200 * 1024) }, 413, "BODY_TOO_LARGE"],
    ]
    for (const [path, init, status, code] of cases) {
      const res = await fetch(`${base}${path}`, init)
      assert.equal(res.status, status, path)
      assert.equal((await res.json()).error, code, path)
    }
  })
})
