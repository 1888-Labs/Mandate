# Public test API

[← Back to the README](../README.md)

A hosted Mandate API with live demo data on Arkiv's Tiramisu testnet. Use it to try every endpoint, or to wire up an integration, before you run your own.

```text
https://mandate-94oq.onrender.com
```

> [!WARNING]
> This is a **testnet sandbox**, not a service. It runs on a free instance with no uptime guarantee, all of its keys are synthetic, and its data expires on Arkiv on **7 November 2026**. For anything real, [self-host](self-hosting.md) it.

## What's in it

One resource, `wallet:treasury-demo`, owned by Alice, with this chain of mandates (all `transfer`, recipient AWS):

```text
Alice ──≤ $1000, delegable──▶ FinanceAgent ──≤ $300──▶ InvoiceAgent
                                   └────────≤ $100 (revoked)──▶ LegacyBot
```

| Principal | Address |
|---|---|
| Alice (trust anchor for `wallet:treasury-demo`) | `0xa090f5ffe81d1603015001563b1d465f6131d2a4` |
| FinanceAgent | `0x21418e3a1c39322e2949f5436966498349a144c4` |
| InvoiceAgent | `0x97e80edf5d361855f64ff09a3d2cbf37b9f7e772` |
| LegacyBot | `0x7f48032178e0c73993546b8c111607001ba4b0dd` |

Every mandate and revocation is a public entity you can inspect on the [Tiramisu explorer](https://tiramisu.explorer.arkiv.network). The full dataset, with entity keys and transaction hashes, is in [`arkiv/demo-dataset.json`](../arkiv/demo-dataset.json).

## Try it

The first request after a quiet period can take up to a minute while the instance wakes up.

```bash
API=https://mandate-94oq.onrender.com
INVOICE=0x97e80edf5d361855f64ff09a3d2cbf37b9f7e772

# Is it up?
curl -s $API/health

# May InvoiceAgent pay AWS $200?  → ALLOW, with the chain Alice → FinanceAgent → InvoiceAgent
curl -s $API/v1/authorize -H 'content-type: application/json' -d '{
  "subject": "'$INVOICE'", "action": "transfer",
  "resource": {"kind": "wallet", "id": "treasury-demo"},
  "amount": 200, "recipient": "AWS" }'

# $700 → DENY AMOUNT_EXCEEDED.  RandomVendor → DENY RECIPIENT_NOT_ALLOWED.
# LegacyBot → DENY REVOKED.

# What does InvoiceAgent hold?
curl -s "$API/v1/agents/$INVOICE/mandates?active=true"

# The full chain behind its mandate, with each link's Arkiv entity and $creator
curl -s $API/v1/mandates/0x614f35e041ade2442e1f925c966ec3c139242f1a833e92c1e67423c471df6527/chain
```

## What you can call

| Endpoint | Available |
|---|---|
| `POST /v1/authorize` | Yes |
| `GET /v1/mandates/{id}`, `/chain`, `GET /v1/agents/{address}/mandates` | Yes |
| `GET /v1/events` | Yes: live Arkiv events for the demo issuers |
| `GET /v1/demo`, `POST /v1/demo/sandbox`, `POST /v1/demo/sandbox/{id}/revoke` | Yes: create a throwaway agent with ≤ $50 for 2 hours, then revoke it. 30 writes per hour in total. |
| `POST /v1/mandates`, `/delegate`, `/revoke` | Only as the demo's issuers. You don't hold their keys, so your own mandates are rejected (`UNTRUSTED_ROOT` or `PUBLISH_REJECTED`). Self-host to publish your own. |

Browser calls are only accepted from the demo's own pages. Call it from a server, `curl`, or a script.

## Watch a revocation take effect

```bash
# 1. Get a fresh sandbox agent with ≤ $50 → AWS
curl -s -X POST $API/v1/demo/sandbox
#    → { "agent": {"address": "0x…"}, "mandate": {"id": "0x…"}, … }

# 2. It may pay AWS $10 → ALLOW
# 3. Revoke it
curl -s -X POST $API/v1/demo/sandbox/<mandate id>/revoke
# 4. The same payment → DENY REVOKED
```

The [web demo](https://1888-labs.github.io/Mandate/demo/) runs exactly these steps, with buttons.
