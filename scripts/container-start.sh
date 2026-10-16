#!/usr/bin/env bash
# Container entrypoint: arkiv-adapter on loopback, mandate-server on $PORT.
# If either process exits, the container exits so the host restarts it.
#
# Required env: ARKIV_PRIVATE_KEY, ARKIV_API_KEY, ARKIV_ISSUER_KEYS,
# DEMO_KEYS, TRUST_ANCHORS. Optional: CORS_ORIGINS, ARKIV_PROJECT_ID.
set -euo pipefail

for name in ARKIV_PRIVATE_KEY ARKIV_ISSUER_KEYS DEMO_KEYS TRUST_ANCHORS; do
  [ -n "${!name:-}" ] || { echo "missing required env: $name" >&2; exit 1; }
done

ADAPTER_PORT=8787
# The adapter is only reachable from inside the container; the token makes
# sure only mandate-server can ask it to write.
TOKEN="$(node -e 'console.log(require("node:crypto").randomBytes(16).toString("hex"))')"

HOST=127.0.0.1 PORT="$ADAPTER_PORT" ADAPTER_TOKEN="$TOKEN" \
  node /app/apps/arkiv-adapter/src/server.ts &

ADAPTER_URL="http://127.0.0.1:$ADAPTER_PORT" ADAPTER_TOKEN="$TOKEN" \
  mandate-server &

wait -n
echo "a process exited; stopping the container" >&2
exit 1
