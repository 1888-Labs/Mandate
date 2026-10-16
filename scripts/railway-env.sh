#!/usr/bin/env bash
# Write the hosted API's environment variables to .mandate-demo/railway.env
# (gitignored), ready to paste into Railway → Variables → Raw Editor.
# Values come from apps/arkiv-adapter/.env and the demo principals created by
# scripts/seed-arkiv.sh. Secrets are written to the file, never printed.
#
#   scripts/railway-env.sh [pages-origin]   # default https://1888-labs.github.io
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="$ROOT/apps/arkiv-adapter/.env"
KEYS="$ROOT/.mandate-demo/keys.json"
OUT="$ROOT/.mandate-demo/railway.env"
ORIGIN="${1:-https://1888-labs.github.io}"
[ -f "$ENV_FILE" ] || { echo "missing $ENV_FILE" >&2; exit 1; }
[ -f "$KEYS" ] || { echo "no demo principals: run scripts/seed-arkiv.sh first" >&2; exit 1; }

python3 - "$ENV_FILE" "$KEYS" "$ROOT/arkiv/demo-dataset.json" "$ORIGIN" "$OUT" <<'PY'
import json, sys
env_file, keys_file, dataset_file, origin, out = sys.argv[1:]
env = {}
for line in open(env_file):
    line = line.strip()
    if line and not line.startswith("#") and "=" in line:
        k, v = line.split("=", 1)
        env[k.strip()] = v.strip().strip('"').strip("'")
keys = json.load(open(keys_file))
alice = json.load(open(dataset_file))["principals"]["alice"]
for name in ("ARKIV_PRIVATE_KEY", "ARKIV_API_KEY"):
    if not env.get(name):
        sys.exit(f"{name} is empty in {env_file}")
names = ("alice", "finance-agent", "invoice-agent", "legacy-bot")
values = {
    "ARKIV_PRIVATE_KEY": env["ARKIV_PRIVATE_KEY"],
    "ARKIV_API_KEY": env["ARKIV_API_KEY"],
    "ARKIV_PROJECT_ID": env.get("ARKIV_PROJECT_ID") or "mandate-global-tour-2026",
    "ARKIV_ISSUER_KEYS": f"{keys['alice']},{keys['finance-agent']}",
    "DEMO_KEYS": ",".join(f"{n}={keys[n]}" for n in names),
    "TRUST_ANCHORS": f"wallet:treasury-demo={alice}",
    "CORS_ORIGINS": origin,
}
with open(out, "w") as f:
    f.writelines(f"{k}={v}\n" for k, v in values.items())
print(f"wrote {len(values)} variables to {out}: " + ", ".join(values))
PY
chmod 600 "$OUT"
