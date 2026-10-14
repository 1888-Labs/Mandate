#!/usr/bin/env bash
# Publish Mandate's long-lived demo dataset to Arkiv Tiramisu, in the main
# namespace, and record the evidence in arkiv/demo-dataset.json.
#
#   Alice ──M_root──▶ FinanceAgent ──M_invoice──▶ InvoiceAgent      (active)
#                                  └─M_legacy───▶ LegacyBot          (revoked)
#
# Arkiv entities always expire, and a mandate's expiry is part of what its
# issuer signed, so "keeping it running" means publishing fresh mandates.
# Re-run this whenever the dataset needs renewing. The demo principals'
# synthetic keys live in .mandate-demo/ (git-ignored), so their addresses
# stay the same across runs and for the deployment.
#
# Usage: scripts/seed-arkiv.sh [lifetime, default 24d]
# Needs: apps/arkiv-adapter/.env with a funded ARKIV_PRIVATE_KEY (and ARKIV_API_KEY).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LIFETIME="${1:-24d}"
PROJECT_ID="${ARKIV_PROJECT_ID:-mandate-global-tour-2026}"
ADAPTER_PORT="${ADAPTER_PORT:-8787}"
SERVER_PORT="${SERVER_PORT:-8080}"
RESOURCE="wallet:treasury-demo"
export MANDATE_HOME="$ROOT/.mandate-demo"
TOKEN="$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
WORK="$(mktemp -d)"
PIDS=()

cleanup() {
  for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
  rm -rf "$WORK"
}
trap cleanup EXIT
say() { printf '\n\033[1m%s\033[0m\n' "$*"; }
die() { printf '\033[31m%s\033[0m\n' "$*" >&2; exit 1; }

cd "$ROOT"
for port in "$ADAPTER_PORT" "$SERVER_PORT"; do
  if curl -s -m 2 -o /dev/null "http://127.0.0.1:$port/health"; then
    die "port $port is already in use. Stop the running adapter/server first."
  fi
done
cargo build -q -p mandate-cli -p mandate-server
M="$ROOT/target/debug/mandate"

say "Demo principals (synthetic keys in .mandate-demo/, git-ignored)"
for name in alice finance-agent invoice-agent legacy-bot; do
  "$M" principal list 2>/dev/null | awk '{print $1}' | grep -qx "$name" || "$M" principal create "$name" >/dev/null
done
"$M" resource own "$RESOURCE" --owner alice >/dev/null
"$M" principal list
addr() { "$M" principal list | awk -v n="$1" '$1 == n { print $2 }'; }
alias_of() { grep -oE 'Mandate M[0-9]+ created' | grep -oE 'M[0-9]+'; }

say "Signing fresh mandates (lifetime $LIFETIME)"
ROOT_M="$("$M" grant --from alice --to finance-agent --action transfer --resource "$RESOURCE" \
  --max-amount 1000 --recipient AWS --delegable --expires-in "$LIFETIME" | alias_of)"
INVOICE_M="$("$M" delegate --mandate "$ROOT_M" --from finance-agent --to invoice-agent \
  --max-amount 300 | alias_of)"
LEGACY_M="$("$M" delegate --mandate "$ROOT_M" --from finance-agent --to legacy-bot \
  --max-amount 100 | alias_of)"
"$M" revoke "$LEGACY_M" --reason retired >/dev/null
for m in "$ROOT_M" "$INVOICE_M" "$LEGACY_M"; do "$M" export "$m" >"$WORK/$m.json"; done
"$M" export "$LEGACY_M" --revocations \
  | python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)[-1]))' >"$WORK/revocation.json"
id_of() { python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["id"])' "$1"; }
echo "  root $ROOT_M, invoice $INVOICE_M, legacy $LEGACY_M (revoked)"

say "Starting arkiv-adapter (namespace $PROJECT_ID) and mandate-server"
ISSUER_KEYS="$(python3 -c 'import json,sys; k=json.load(open(sys.argv[1])); print(k["alice"]+","+k["finance-agent"])' "$MANDATE_HOME/keys.json")"
ARKIV_PROJECT_ID="$PROJECT_ID" PORT="$ADAPTER_PORT" ADAPTER_TOKEN="$TOKEN" ARKIV_ISSUER_KEYS="$ISSUER_KEYS" \
  node --env-file=apps/arkiv-adapter/.env apps/arkiv-adapter/src/server.ts >"$WORK/adapter.log" 2>&1 &
PIDS+=($!)
ADAPTER_URL="http://127.0.0.1:$ADAPTER_PORT" ADAPTER_TOKEN="$TOKEN" PORT="$SERVER_PORT" \
  TRUST_ANCHORS="$RESOURCE=$(addr alice)" RUST_LOG=warn \
  "$ROOT/target/debug/mandate-server" >"$WORK/server.log" 2>&1 &
PIDS+=($!)
API="http://127.0.0.1:$SERVER_PORT"
for _ in $(seq 1 60); do
  curl -sf -m 2 "$API/health" >/dev/null && curl -sf -m 20 "http://127.0.0.1:$ADAPTER_PORT/health" >/dev/null && break
  sleep 1
done
for pid in "${PIDS[@]}"; do
  kill -0 "$pid" 2>/dev/null || { cat "$WORK/adapter.log" "$WORK/server.log" >&2; die "a service failed to start"; }
done

# post NAME PATH FILE: publish, require success, keep the response.
post() {
  curl -s -m 180 -X POST -H 'content-type: application/json' "$API$2" --data-binary "@$3" >"$WORK/$1.out"
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit("error" in d and d["error"])' "$WORK/$1.out" \
    || die "publishing $1 failed: $(cat "$WORK/$1.out")"
  echo "  $1: $(python3 -c 'import json,sys; p=json.load(open(sys.argv[1]))["published"]; print(p.get("entityKey"), "tx", p.get("txHash"))' "$WORK/$1.out")"
}

say "Publishing to Arkiv (each from its issuer's own wallet)"
post root /v1/mandates "$WORK/$ROOT_M.json"
post invoice "/v1/mandates/$(id_of "$WORK/$ROOT_M.json")/delegate" "$WORK/$INVOICE_M.json"
post legacy "/v1/mandates/$(id_of "$WORK/$ROOT_M.json")/delegate" "$WORK/$LEGACY_M.json"
post revocation "/v1/mandates/$(id_of "$WORK/$LEGACY_M.json")/revoke" "$WORK/revocation.json"

say "Checking the published dataset"
check() { # check AGENT AMOUNT EXPECTED
  local got
  got="$(curl -s -m 180 -X POST -H 'content-type: application/json' "$API/v1/authorize" -d "{
    \"subject\": \"$(addr "$1")\", \"action\": \"transfer\",
    \"resource\": {\"kind\": \"wallet\", \"id\": \"treasury-demo\"}, \"amount\": $2, \"recipient\": \"AWS\" }" \
    | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("reason") or d.get("decision") or d.get("error"))')"
  [ "$got" = "$3" ] || die "$1 \$$2 → AWS: expected $3, got $got"
  echo "  ✓ $1 \$$2 → AWS: $got"
}
check invoice-agent 200 ALLOW
check invoice-agent 700 AMOUNT_EXCEEDED
check legacy-bot 50 REVOKED

say "Writing arkiv/demo-dataset.json"
python3 - "$WORK" "$PROJECT_ID" "$ROOT_M" "$INVOICE_M" "$LEGACY_M" <<'PY' >"$ROOT/arkiv/demo-dataset.json"
import json, sys, datetime as dt
work, project, root_m, inv_m, leg_m = sys.argv[1:]
doc = lambda n: json.load(open(f"{work}/{n}"))
out = lambda n: json.load(open(f"{work}/{n}.out"))["published"]
keys = json.load(open(".mandate-demo/state.json"))["principals"]
name = {v: k for k, v in keys.items()}
utc = lambda t: dt.datetime.fromtimestamp(t, dt.UTC).strftime("%Y-%m-%d %H:%M UTC")
def entry(label, file, published, status):
    d = doc(file)
    return {
        "label": label, "status": status, "mandateId": d["id"],
        "issuer": f'{name[d["issuer"]]} {d["issuer"]}', "subject": f'{name[d["subject"]]} {d["subject"]}',
        "limits": d["constraints"], "expiresAt": utc(d["expiresAt"]),
        "entityKey": published["entityKey"], "txHash": published["txHash"],
    }
rev = doc("revocation.json")
print(json.dumps({
    "network": "Arkiv Tiramisu (chain 7738577)",
    "projectId": project,
    "createdAt": utc(int(dt.datetime.now(dt.UTC).timestamp())),
    "trustAnchor": {"resource": "wallet:treasury-demo", "owner": f'alice {keys["alice"]}'},
    "principals": keys,
    "mandates": [
        entry("Alice → FinanceAgent: ≤ $1000 to AWS, delegable", f"{root_m}.json", out("root"), "active"),
        entry("FinanceAgent → InvoiceAgent: ≤ $300 to AWS", f"{inv_m}.json", out("invoice"), "active"),
        entry("FinanceAgent → LegacyBot: ≤ $100 to AWS", f"{leg_m}.json", out("legacy"), "revoked"),
    ],
    "revocations": [{
        "revocationId": rev["id"], "revokes": rev["mandateId"], "reason": rev["reason"],
        "issuer": f'{name[rev["issuer"]]} {rev["issuer"]}',
        "entityKey": out("revocation")["entityKey"], "txHash": out("revocation")["txHash"],
        "owner": "0x000000000000000000000000000000000000dEaD (burn address: undeletable)",
    }],
}, indent=2, ensure_ascii=False))
PY
say "Done. Dataset is live on Arkiv until $(python3 -c 'import json;print(json.load(open("arkiv/demo-dataset.json"))["mandates"][0]["expiresAt"])')."
