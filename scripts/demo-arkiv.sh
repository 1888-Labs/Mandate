#!/usr/bin/env bash
# The full Mandate demo against Arkiv Tiramisu, through the Rust HTTP API:
#
#   CLI (signs locally) → mandate-server (verifies) → arkiv-adapter → Arkiv
#
# Usage: scripts/demo-arkiv.sh
# Needs: cargo, node ≥ 22.18, curl, python3, and apps/arkiv-adapter/.env with
# a funded ARKIV_PRIVATE_KEY. Writes to ARKIV_PROJECT_ID (default: the
# acceptance namespace, so the public demo namespace stays clean).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT_ID="${ARKIV_PROJECT_ID:-mandate-global-tour-2026-acceptance}"
ADAPTER_PORT="${ADAPTER_PORT:-8787}"
SERVER_PORT="${SERVER_PORT:-8080}"
TOKEN="$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
WORK="$(mktemp -d)"
export MANDATE_HOME="$WORK/home"
PIDS=()

cleanup() {
  for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
  rm -rf "$WORK"
}
trap cleanup EXIT

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }

cd "$ROOT"
cargo build -q -p mandate-cli -p mandate-server
M="$ROOT/target/debug/mandate"

say "Principals, signed locally by the CLI"
for name in alice finance-agent invoice-agent; do "$M" principal create "$name" >/dev/null; done
"$M" principal list
addr() { "$M" principal list | awk -v n="$1" '$1 == n { print $2 }'; }
ALICE="$(addr alice)"
INVOICE="$(addr invoice-agent)"

"$M" resource own wallet:treasury-demo --owner alice >/dev/null
"$M" grant --from alice --to finance-agent --action transfer --resource wallet:treasury-demo \
  --max-amount 1000 --recipient AWS --delegable --expires-in 2h >/dev/null
"$M" delegate --mandate M1 --from finance-agent --to invoice-agent \
  --max-amount 300 --expires-in 1h >/dev/null
"$M" export M1 >"$WORK/m1.json"
"$M" export M2 >"$WORK/m2.json"
M1_ID="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["id"])' "$WORK/m1.json")"

say "Starting arkiv-adapter (namespace $PROJECT_ID) and mandate-server"
# Run node directly (not in a subshell) so $! is its PID and cleanup stops it.
ARKIV_PROJECT_ID="$PROJECT_ID" PORT="$ADAPTER_PORT" ADAPTER_TOKEN="$TOKEN" \
  node --env-file=apps/arkiv-adapter/.env apps/arkiv-adapter/src/server.ts >"$WORK/adapter.log" 2>&1 &
PIDS+=($!)
ADAPTER_URL="http://127.0.0.1:$ADAPTER_PORT" ADAPTER_TOKEN="$TOKEN" PORT="$SERVER_PORT" \
  TRUST_ANCHORS="wallet:treasury-demo=$ALICE" RUST_LOG=warn \
  "$ROOT/target/debug/mandate-server" >"$WORK/server.log" 2>&1 &
PIDS+=($!)
API="http://127.0.0.1:$SERVER_PORT"
for _ in $(seq 1 60); do
  curl -sf -m 2 "$API/health" >/dev/null && curl -sf -m 20 "http://127.0.0.1:$ADAPTER_PORT/health" >/dev/null && break
  sleep 1
done

if ! curl -sf -m 20 "http://127.0.0.1:$ADAPTER_PORT/health" >/dev/null; then
  echo "adapter did not start:"; cat "$WORK/adapter.log"; exit 1
fi

post() { curl -s -m 120 -X POST -H 'content-type: application/json' "$API$1" --data-binary "@$2"; echo; }
check() {
  curl -s -m 120 -X POST -H 'content-type: application/json' "$API/v1/authorize" -d "{
    \"subject\": \"$INVOICE\", \"action\": \"transfer\",
    \"resource\": {\"kind\": \"wallet\", \"id\": \"treasury-demo\"},
    \"amount\": $1, \"recipient\": \"$2\" }" | python3 -m json.tool
}

say "1. Alice → FinanceAgent: ≤ \$1000 to AWS (M1), stored on Arkiv"
post /v1/mandates "$WORK/m1.json"

say "2. FinanceAgent → InvoiceAgent: ≤ \$300 to AWS (M2), stored on Arkiv"
post "/v1/mandates/$M1_ID/delegate" "$WORK/m2.json"

say "3. InvoiceAgent: \$200 → AWS"
check 200 AWS

say "4. InvoiceAgent: \$700 → AWS"
check 700 AWS

say "5. InvoiceAgent: \$100 → RandomVendor"
check 100 RandomVendor

say "6. Alice revokes M1 (signed locally, stored on Arkiv, undeletable)"
"$M" revoke M1 >/dev/null
"$M" export M1 --revocations | python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)[0]))' >"$WORK/rev.json"
post "/v1/mandates/$M1_ID/revoke" "$WORK/rev.json"

say "7. InvoiceAgent: \$100 → AWS"
check 100 AWS

say "Done. Every record above is public on Arkiv Tiramisu under project_id=$PROJECT_ID."
