#!/usr/bin/env bash
# Run the Mandate web app locally against the live demo dataset on Arkiv:
# the Arkiv adapter plus mandate-server with the web UI and demo mode.
#
#   scripts/run-local.sh            # then open http://127.0.0.1:8080
#
# Needs apps/arkiv-adapter/.env (funded ARKIV_PRIVATE_KEY, ARKIV_API_KEY) and
# the demo principals from scripts/seed-arkiv.sh (.mandate-demo/keys.json).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT_ID="${ARKIV_PROJECT_ID:-mandate-global-tour-2026}"
ADAPTER_PORT="${ADAPTER_PORT:-8787}"
PORT="${PORT:-8080}"
KEYS="$ROOT/.mandate-demo/keys.json"
[ -f "$KEYS" ] || { echo "no demo principals: run scripts/seed-arkiv.sh first" >&2; exit 1; }

cd "$ROOT"
cargo build -q -p mandate-server

keys() { python3 -c 'import json,sys; k=json.load(open(sys.argv[1])); print(sys.argv[2].format(**{n.replace("-","_"): v for n,v in k.items()}))' "$KEYS" "$1"; }
alice_addr() { "$ROOT/target/debug/mandate" --home "$ROOT/.mandate-demo" principal list | awk '$1=="alice"{print $2}'; }
cargo build -q -p mandate-cli
TOKEN="$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
PIDS=()
trap 'for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null || true; done' EXIT INT TERM

ARKIV_PROJECT_ID="$PROJECT_ID" PORT="$ADAPTER_PORT" ADAPTER_TOKEN="$TOKEN" \
  ARKIV_ISSUER_KEYS="$(keys '{alice},{finance_agent}')" \
  node --env-file=apps/arkiv-adapter/.env apps/arkiv-adapter/src/server.ts &
PIDS+=($!)

ADAPTER_URL="http://127.0.0.1:$ADAPTER_PORT" ADAPTER_TOKEN="$TOKEN" PORT="$PORT" \
  TRUST_ANCHORS="wallet:treasury-demo=$(alice_addr)" \
  DEMO_KEYS="$(keys 'alice={alice},finance-agent={finance_agent},invoice-agent={invoice_agent},legacy-bot={legacy_bot}')" \
  "$ROOT/target/debug/mandate-server" &
PIDS+=($!)

echo "Mandate is starting on http://127.0.0.1:$PORT (Ctrl-C to stop)"
wait
