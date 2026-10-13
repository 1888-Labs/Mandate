# CLI

[← Back to the README](../README.md)

The `mandate` command signs mandates and revocations locally and checks requests. Install it with:

```bash
cargo install --path crates/mandate-cli
```

The whole demo runs locally. State lives in `./.mandate/` (override with `--home` or `MANDATE_HOME`). This exact flow is an automated test ([`cli_demo.rs`](../crates/mandate-cli/tests/cli_demo.rs)).

```bash
mandate principal create alice
mandate principal create finance-agent
mandate principal create invoice-agent

# Alice owns the treasury wallet, so only she can start a chain for it.
mandate resource own wallet:treasury-demo --owner alice

mandate grant --from alice --to finance-agent \
  --action transfer --resource wallet:treasury-demo \
  --max-amount 1000 --recipient AWS --delegable --expires-in 48h

mandate delegate --mandate M1 --from finance-agent --to invoice-agent \
  --max-amount 300 --expires-in 24h
```

```text
Delegation valid. Mandate M2 created

  alice
   └── finance-agent  [≤ $1000 → AWS]
        └── invoice-agent  [≤ $300 → AWS]
```

```bash
mandate check --agent invoice-agent --action transfer \
  --resource wallet:treasury-demo --amount 200 --recipient AWS
```

```text
✓ AUTHORIZED

  Agent        invoice-agent
  Request      transfer $200 → AWS  on wallet:treasury-demo
  Authority    alice
  Delegation   alice → finance-agent → invoice-agent
  Limits       ≤ $300 → AWS
  Mandate      M2
```

The same check with `--amount 700` gives `✗ DENIED  AMOUNT_EXCEEDED` (requested $700, maximum $300), and `--recipient RandomVendor` gives `✗ DENIED  RECIPIENT_NOT_ALLOWED`. Trying to delegate *more* than you hold is refused before anything is stored:

```text
✗ DELEGATION REJECTED  AMOUNT_ESCALATION

  child amount limit exceeds the parent's.
```

Now Alice revokes her grant:

```bash
mandate revoke M1
mandate check --agent invoice-agent --action transfer \
  --resource wallet:treasury-demo --amount 100 --recipient AWS
mandate chain M2
```

```text
✗ DENIED  ANCESTOR_REVOKED

  Agent        invoice-agent
  Request      transfer $100 → AWS  on wallet:treasury-demo
  Revoked      M1 by alice (an ancestor)
  Revocation   0x1f9b1537de358d771a37e9e00b0e4273ae4e4a5caea8c35f12a28a3e2242e0f0
  Delegation   alice → finance-agent → invoice-agent
  Mandate      M2

Chain for M2

  alice
   │  M1  transfer ≤ $1000 → AWS  delegable  expires 2026-10-15 00:47 UTC
   │      ✗ revoked by alice (user_revoked)
   ▼
  finance-agent
   │  M2  transfer ≤ $300 → AWS  expires 2026-10-14 00:47 UTC
   │      ✓ valid
   ▼
  invoice-agent

  Result       ✗ chain broken: M1 is not usable, so M2 grants nothing
```

Other commands: `mandate inspect M2`, `mandate principal list` and `mandate resource list`. Mandates can be referred to as `M1`, by full ID, or by a unique ID prefix. **Exit codes:** `0` success or ALLOW, `1` DENY or rejected delegation, `2` error, so `mandate check` can gate a shell script.

> [!WARNING]
> `.mandate/keys.json` holds **plaintext development private keys** (written owner-only). It's for local demos only and is git-ignored. Never use these keys for anything real.

To publish what the CLI signs to Arkiv, use `mandate export M1` (prints the exact signed document) with the [HTTP API](api.md), or run [`scripts/demo-arkiv.sh`](../scripts/demo-arkiv.sh), which does the whole flow.
