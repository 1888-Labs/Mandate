#!/usr/bin/env bash
# Container entrypoint. See docs/self-hosting.md and docs/configuration.md.
#
#   MANDATE_BACKEND=arkiv (default)  arkiv-adapter on loopback + mandate-server on $PORT
#   MANDATE_BACKEND=memory           mandate-server alone, state in memory (for trials)
#
# Nothing is strictly required. Without TRUST_ANCHORS no root mandate is
# trusted, so every check is DENY; without ARKIV_ISSUER_KEYS the adapter is
# read-only (authorize and reads work, publishing doesn't). If either process
# exits, the container exits so the host restarts it.
set -euo pipefail

warn() { echo "container-start: $*" >&2; }

[ -n "${TRUST_ANCHORS:-}" ] || warn "TRUST_ANCHORS is empty: no root mandate is trusted, so every check is DENY"

if [ "${MANDATE_BACKEND:-arkiv}" = "memory" ]; then
  warn "in-memory backend: nothing is read from or written to Arkiv, and state is lost on restart"
  exec mandate-server
fi

[ -n "${ARKIV_API_KEY:-}" ] || warn "ARKIV_API_KEY is empty: the anonymous Arkiv query budget is small"
[ -n "${ARKIV_ISSUER_KEYS:-}" ] || warn "ARKIV_ISSUER_KEYS is empty: read-only, this server cannot publish mandates"

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
