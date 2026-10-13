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

**Workaround:** create the revocation and, in the same `executeBatch`, transfer it to the burn address `0x…dEaD`. The new entity's key is computed beforehand with `getEntityNonce` + `predictEntityKey` and the same `salt`. The adapter checks that the key it predicted matches `createdEntities[0]`. *Whether the burn owner really prevents deletion is checked on-chain by `npm run test:arkiv` (pending a funded wallet).*

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

## Positive notes

- **`ExpirationTime.atDate(date)`** maps exactly onto a mandate's signed `expiresAt`. "Authorization expiry ≈ entity lifetime" took one line.
- **`predictEntityKey` + `executeBatch`** make "create then transfer ownership" atomic, without a second round trip.
- **Typed `select()` projections**: selecting only `key/owner/payload/attributes` keeps queries small, and the compile-time narrowing caught a missing field.
- **TypeScript source ships in the npm package.** Reading `src/` answered every question the README didn't (attribute types, flags, batch semantics) without leaving the editor.
- **Read-only access needs no setup:** the public client queried Tiramisu on the first try (`scripts/probe.ts`).
