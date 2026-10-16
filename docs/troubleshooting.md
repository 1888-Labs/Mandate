# Troubleshooting

[← Back to the README](../README.md)

Common problems when running Mandate, and how to fix them. If yours isn't here, [open an issue](https://github.com/1888-Labs/Mandate/issues) with the server logs (they never contain keys).

## Every check is `DENY UNTRUSTED_ROOT`

The chain doesn't start with a trusted owner of the resource. Check that:

- `TRUST_ANCHORS` is set at all. The container warns at startup if it's empty.
- The resource is written exactly as in your requests: `wallet:treasury` is not `wallet:treasury-demo`.
- The address is the **root issuer's**, not the agent's. `GET /v1/mandates/{id}/chain` shows who that is.

## Every check is `DENY NO_MANDATE`

The server found no mandate for this subject, action and resource.

- **Wrong namespace:** `ARKIV_PROJECT_ID` must be the one the mandates were published under.
- **Wrong subject:** the address must be the agent's, as on the mandate.
- **Not published by its issuer:** the server ignores copies on Arkiv created by anyone other than the mandate's issuer. `GET /v1/agents/{address}/mandates` lists what it can see.

## `DENY AMOUNT_EXCEEDED` or `RECIPIENT_NOT_ALLOWED` with no amount or recipient

If any mandate in the chain sets `maxAmount` or `allowedRecipients`, requests must include `amount` or `recipient`. Leaving them out fails closed.

## `503 STATE_UNAVAILABLE`

The server couldn't read Arkiv, so it refuses to decide. Your tool must treat this as a no.

- **No or exhausted API key:** set `ARKIV_API_KEY`. The anonymous budget is small, and every decision makes queries. The logs show `Arkiv rate limit (retry after …s)`.
- **RPC unreachable:** check `ARKIV_RPC_URL`, or remove it to use the default.
- **The adapter died:** in the container, the server and adapter exit together and your platform restarts them. Check the logs for the adapter's error.

## `422 PUBLISH_REJECTED` with `ISSUER_KEY_UNAVAILABLE`

The server is read-only for that issuer: `ARKIV_ISSUER_KEYS` doesn't contain the key of the mandate's issuer. Add the key, or publish from wherever the issuer's key lives. See [Publishing through your server](self-hosting.md#publishing-through-your-server).

## The adapter crashes with `invalid private key, expected hex or 32 bytes`

A key is malformed. Each key must be 64 hex characters, with or without `0x`, and keys in `ARKIV_ISSUER_KEYS` must be separated by commas with no quotes. Older versions required the `0x` prefix.

## The container exits right after starting

Read the first error in the logs. Usually it's one of:

- `TRUST_ANCHORS: …`: the format is wrong. It should be `kind:id=0xaddr[,0xaddr];…`.
- `CORS_ORIGINS: …`: an origin isn't a valid header value. Use full origins like `https://app.example.com`, with no path or trailing slash.
- `bind 0.0.0.0:…`: the port is taken, or your platform expects a different `PORT`.

## The browser says the API is unreachable, but `curl` works

That's CORS. Add your page's exact origin (scheme, host and port) to `CORS_ORIGINS`, comma-separated, and restart. Server-to-server calls don't need it.

## The first request takes about a minute

The host put the instance to sleep. Render's free tier does this after 15 minutes without traffic. Use a paid instance, keep a machine always running, or ping `/health` every few minutes.

## The live feed in the web UI says "Offline"

`/v1/events` needs the Arkiv backend, and a proxy that doesn't buffer responses. Behind nginx, add `proxy_buffering off;` for `/v1/events`. The in-memory backend has no live events.

## `400 INVALID_REQUEST: unknown field timestamp`

The server sets the time itself, so agents can't backdate requests past a mandate's expiry. Remove `timestamp` and any other extra fields from `/v1/authorize` requests.
