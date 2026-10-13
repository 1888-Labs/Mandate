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
for port in "$ADAPTER_PORT" "$SERVER_PORT"; do
  if curl -s -m 2 -o /dev/null "http://127.0.0.1:$port/health"; then
    echo "port $port is already in use (a previous adapter/server?). Stop it first." >&2
    exit 1
  fi
done
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
# Issuers publish from their own wallets, so Arkiv's $creator is the issuer.
# Hand the adapter Alice's and FinanceAgent's local development keys; the
# adapter tops their wallets up with gas from its funder wallet.
ISSUER_KEYS="$(python3 -c 'import json,sys; k=json.load(open(sys.argv[1])); print(k["alice"]+","+k["finance-agent"])' "$MANDATE_HOME/keys.json")"
# Run node directly (not in a subshell) so $! is its PID and cleanup stops it.
ARKIV_PROJECT_ID="$PROJECT_ID" PORT="$ADAPTER_PORT" ADAPTER_TOKEN="$TOKEN" ARKIV_ISSUER_KEYS="$ISSUER_KEYS" \
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

for pid in "${PIDS[@]}"; do
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "a service failed to start:" >&2
    cat "$WORK/adapter.log" "$WORK/server.log" >&2
    exit 1
  fi
done

FAILED=0
fail() { printf '\033[31m✗ %s\033[0m\n' "$*"; FAILED=1; }

# post PATH FILE: publish a signed document; it must be accepted.
post() {
  local body
  body="$(curl -s -m 120 -X POST -H 'content-type: application/json' "$API$1" --data-binary "@$2")"
  echo "$body"
  python3 -c 'import json,sys; sys.exit("error" in json.loads(sys.argv[1]))' "$body" || fail "publish rejected"
}

# check AMOUNT RECIPIENT EXPECTED: EXPECTED is ALLOW or a deny reason code.
check() {
  local body got
  body="$(curl -s -m 120 -X POST -H 'content-type: application/json' "$API/v1/authorize" -d "{
    \"subject\": \"$INVOICE\", \"action\": \"transfer\",
    \"resource\": {\"kind\": \"wallet\", \"id\": \"treasury-demo\"},
    \"amount\": $1, \"recipient\": \"$2\" }")"
  echo "$body" | python3 -m json.tool
  got="$(python3 -c 'import json,sys; d=json.loads(sys.argv[1]); print(d.get("reason") or d.get("decision") or d.get("error"))' "$body")"
  [ "$got" = "$3" ] || fail "expected $3, got $got"
}

say "1. Alice → FinanceAgent: ≤ \$1000 to AWS (M1), stored on Arkiv"
post /v1/mandates "$WORK/m1.json"

say "2. FinanceAgent → InvoiceAgent: ≤ \$300 to AWS (M2), stored on Arkiv"
post "/v1/mandates/$M1_ID/delegate" "$WORK/m2.json"

say "3. InvoiceAgent: \$200 → AWS"
check 200 AWS ALLOW

say "4. InvoiceAgent: \$700 → AWS"
check 700 AWS AMOUNT_EXCEEDED

say "5. InvoiceAgent: \$100 → RandomVendor"
check 100 RandomVendor RECIPIENT_NOT_ALLOWED

say "6. Alice revokes M1 (signed locally, stored on Arkiv, undeletable)"
"$M" revoke M1 >/dev/null
"$M" export M1 --revocations | python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)[0]))' >"$WORK/rev.json"
post "/v1/mandates/$M1_ID/revoke" "$WORK/rev.json"

say "7. InvoiceAgent: \$100 → AWS"
check 100 AWS ANCESTOR_REVOKED

if [ "$FAILED" -ne 0 ]; then
  say "Demo FAILED: see the ✗ lines above."
  exit 1
fi
say "All 7 steps behaved as expected. Every record above is public on Arkiv Tiramisu under project_id=$PROJECT_ID."
