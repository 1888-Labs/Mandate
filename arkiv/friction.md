# Arkiv friction log

Real issues hit while building Mandate on `@arkiv-network/sdk` 0.8.1 / Tiramisu, recorded as they happened. Positive notes are at the end.

---

## 1. u64 values in JSON payloads lose precision in JavaScript

*2026-10-13*

**Goal:** store signed mandates as JSON payloads and read them back unchanged.

**Expected:** a JSON document stored and fetched back is identical.

**Actual:** Mandate's Rust CLI uses nanosecond nonces (`1791852453273973432`), which are above 2⁵³. The SDK's documented path, `jsonToPayload(obj)` (implemented as `toBytes(JSON.stringify(json))`), requires a parsed JS object. Parsing already rounds the nonce, so `String(JSON.parse(raw).nonce) !== "1791852453273973432"`. For a signed document that is fatal: the content no longer matches its hash, so the ID and signature break. Nothing fails loudly.

**SDK/API details:** `jsonToPayload(json: object)` in `@arkiv-network/sdk/utils`. Reading back with `entity.toJson()` has the same problem in reverse.

**Workaround:** the adapter never parses-then-stringifies a payload. It writes the request body bytes as received (`stringToBytes(raw)`) and splices the stored text into responses verbatim (`entity.toText()`). A unit test pins this with a real CLI-produced mandate.

**Suggested improvement:** note in the `jsonToPayload` / `toJson` docs that JSON numbers above 2⁵³ aren't preserved. Consider a `rawJsonPayload(text)` helper, or a `toJson({ bigint: true })` option.

---

## 2. No way to make an entity undeletable; `readonly` doesn't prevent deletion

*2026-10-13*

**Goal:** publish revocations that nobody can remove. If a revocation can be deleted, the revoked mandate comes back to life.

**Expected:** a creation flag for append-only / undeletable records, since `readonly` sounded close.

**Actual:** `readonly` "can never be changed — only its expiry extended, its ownership transferred, and the entity deleted" (`src/entity/flags.ts`). The owner can always delete. For an audit or revocation log, that means the operator's wallet has the power to undo revocations.

**SDK/API details:** `CreationFlags = { readonly?, permissionlessExtension? }`. No flag covers deletion.

**Workaround:** create the revocation and, in the same `executeBatch`, transfer it to the burn address `0x…dEaD`. The new entity's key is computed beforehand with `getEntityNonce` + `predictEntityKey` and the same `salt`. The adapter checks that the key it predicted matches `createdEntities[0]`. **Verified on Tiramisu:** deleting the burn-owned revocation from the adapter wallet fails with *"entity … is owned by 0x…dEaD, not 0xcE64…"*, while an entity the adapter still owns can be deleted.

**Suggested improvement:** an `undeletable` (or `appendOnly`) creation flag. Failing that, a documented "renounce ownership" operation, rather than relying on a well-known burn address.

---

## 3. Getting testnet funds: no faucet link in the SDK README

*2026-10-13*

**Goal:** fund a fresh wallet to write the first entity.

**Expected:** the SDK README's write tutorial links a faucet.

**Actual:** the README says to "fund its address on the Arkiv Tiramisu testnet" and offers a shared sample private key ("funds may not always be available"), but gives no faucet URL. A web search for "Arkiv Tiramisu faucet" found nothing relevant. The faucet (`https://hub.arkiv.network/faucet`) was found via the arkiv.network homepage.

**Workaround:** linked the faucet in our README and setup instructions.

**Suggested improvement:** link the faucet from the npm README next to the write example. The shared sample key also nudges people towards using a shared, public private key; it might be better to drop it.

---

## 4. Minor: README install snippet shows an old version

*2026-10-13*

The npm README for 0.8.1 shows the resulting dependency as `"@arkiv-network/sdk": "^0.6.0"`. It's harmless but confusing when checking you have the current release.

---

## 5. Intermittent `fetch failed` from Node on a slow link

*2026-10-13*

**Goal:** run a ~4-minute on-chain test that polls the RPC every 20 s.

**Expected:** RPC calls succeed; `curl` to the same endpoint answered in about 1 s.

**Actual:** `HttpRequestError: HTTP request failed … fetch failed` partway through, twice. The underlying causes were `connect ETIMEDOUT` (IPv4) and `ENETUNREACH` (IPv6). Measured over 25 sequential `eth_blockNumber` calls from Node 24:

| Node setting | ok | failed |
|---|---|---|
| default | 9 | 16 (`ETIMEDOUT` ×13, `EAI_AGAIN` ×3) |
| `--network-family-autoselection-attempt-timeout=2000` | 25 | 0 |
| `--dns-result-order=ipv4first` | 25 | 0 |

Node's "happy eyeballs" gives each address only **250 ms** to connect. On a high-latency connection the TLS endpoint often takes longer, so every address times out. This is a Node default rather than an Arkiv bug, but it hits the default `http()` transport the SDK examples use, and viem's built-in retry didn't help.

**Workaround:** the adapter calls `net.setDefaultAutoSelectFamilyAttemptTimeout(2000)` at startup and configures `http(url, { retryCount: 3, timeout: 30_000 })`.

**Suggested improvement:** a troubleshooting note in the SDK docs for `fetch failed` / `ETIMEDOUT` that mentions this setting would save time for anyone on mobile, satellite or far-away connections.

---

## 6. Anonymous query budget runs out, and the SDK then appears to hang

*2026-10-13*

**Goal:** run the Mandate server against Arkiv. Each authorization decision needs a few queries (the agent's candidate mandates, their ancestors, their revocations).

**Expected:** either queries succeed, or a quota error comes back promptly.

**Actual:** after a day of development (acceptance tests, a 4-minute expiry poll, the end-to-end demo), every `arkiv_query` started failing:

```text
HTTP/2 429
ratelimit: limit=10000, remaining=0, reset=609
retry-after: 609
{"error":"ANON_COST_LIMITED","message":"unauthenticated query budget exhausted; get an API key at https://hub.arkiv.network for a monthly quota"}
```

The server's message is excellent. But through the SDK we never saw it: viem's `http()` transport honours `Retry-After`, so each of its retries silently slept about 10 minutes. Our adapter's requests just hung (a curl to it timed out at 60 s, and a direct SDK call was still pending after 150 s). Plain `eth_chainId` kept working, which made it look like a bug in our query code. We found the cause only by sending a raw `arkiv_query` with curl.

**SDK/API details:** `@arkiv-network/sdk` 0.8.1 doesn't mention API keys, quotas or rate limits anywhere in its README or source. The docs' Access Keys page (`docs.arkiv.network/start-here/access-keys/`) explains how to send a key (URL path, `X-API-KEY` header or `Authorization: Bearer`) but gives no numbers for the anonymous limit or the keyed quota.

**Workaround:**
- A custom `fetchFn` on the viem transport turns an HTTP 429 into an immediate `ArkivRateLimited` error carrying `retryAfter`. The adapter answers `503 ARKIV_RATE_LIMITED` in under a second. (Test: against a fake RPC answering `429 / Retry-After: 609`, the call fails in about 0.5 s.)
- `ARKIV_API_KEY` support (sent as `X-API-KEY`, so the key never appears in URLs or logs).
- Spend less: one OR query per chain level for ancestors, one OR query for all revocations, and attributes fetched only when needed.

With an access key the same queries succeed (`ratelimit: limit=9999, remaining=9998`), and the full demo passes.

**Suggested improvement:**
- Mention access keys and the anonymous budget in the SDK README next to `createPublicClient`, with an `apiKey` option.
- Have the SDK surface `ANON_COST_LIMITED` / `ratelimit` as a typed error rather than leaving viem to sleep on `Retry-After`.
- Publish the anonymous limit and how `cost units` are computed per query.

---

## 7. Tested path: per-mandate `$creator` filters inside one OR query (worked first time)

*2026-10-13 · `@arkiv-network/sdk` 0.8.1, Tiramisu*

**Goal:** look up the revocations of several mandates in one query, each only if created by *that mandate's* issuer, so entities from anyone else can't crowd out a real revocation.

**What we tried:** `createdBy()` takes a single address, so it can't express a different creator per mandate. Instead we put `eq("$creator", addr(issuer))` inside the filter tree, the same thing `createdBy()` does internally (`src/query/queryBuilder.ts`):

```ts
.where(and(
  eq("project_id", str(projectId)), eq("entity_type", str("revocation")),
  or(
    and(eq("mandate_id", bytes32(a)), eq("$creator", addr(issuerA))),
    and(eq("mandate_id", bytes32(b)), eq("$creator", addr(issuerB))),
  ),
))
```

**Expected:** only revocations of A created by issuerA and of B created by issuerB.

**Actual:** exactly that, on the first attempt. To check it, `npm run test:arkiv` (step 8) has a *different* wallet create a fake "revocation of M1 by Alice" with identical attributes. A plain `mandate_id` query returns the fake; the `$creator` query does not.

**Reproduce:** `cargo build -p mandate-cli && cd apps/arkiv-adapter && npm run test:arkiv` (needs a funded `ARKIV_PRIVATE_KEY`).

**Suggestion:** document that `$creator` / `$owner` can be used in `eq(...)` anywhere in an expression, not only through `createdBy()` / `ownedBy()`. It's what makes per-row authorship filters possible, and it isn't obvious from the README.

---

## Positive notes

- **`ExpirationTime.atDate(date)`** maps exactly onto a mandate's signed `expiresAt`. "Authorization expiry ≈ entity lifetime" took one line.
- **`predictEntityKey` + `executeBatch`** make "create then transfer ownership" atomic, without a second round trip.
- **Typed `select()` projections**: selecting only `key/owner/payload/attributes` keeps queries small, and the compile-time narrowing caught a missing field.
- **TypeScript source ships in the npm package.** Reading `src/` answered every question the README didn't (attribute types, flags, batch semantics) without leaving the editor.
- **Read-only access needs no setup:** the public client queried Tiramisu on the first try (`scripts/probe.ts`).
- **Expiry is prompt:** a 3-minute entity left live queries at most 19 s after its deadline, which is close enough that Arkiv expiry tracks authorization expiry in practice.
- **Clear revert messages:** a forbidden delete says exactly who owns the entity, and a forbidden patch says the entity is read-only and what is still allowed. That made it easy to prove the protections work for the right reason.
