#!/usr/bin/env bash
# Build mandate-warden and (re)deploy it as a service on the Warden Sprite.
# Reads WARDEN_ADMIN_TOKEN, GITHUB_CONNECTION_ID and SPRITES_TOKEN from .env.sprites (gitignored).
set -euo pipefail
cd "$(dirname "$0")/.."
WARDEN_SPRITE="${WARDEN_SPRITE:-mandate-warden}"
AGENT_SPRITE="${AGENT_SPRITE:-mandate-agent}"
set -a; . ./.env.sprites; set +a

cargo build --release -p mandate-warden
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
gzip -9 -c target/release/mandate-warden > "$tmp/w.gz"
gzip -9 -c target/release/warden-call > "$tmp/c.gz"

echo "uploading warden to $WARDEN_SPRITE"
sprite exec --stdin -s "$WARDEN_SPRITE" -- bash -c 'mkdir -p ~/bin && gunzip -c > ~/bin/mandate-warden.new && chmod +x ~/bin/mandate-warden.new' < "$tmp/w.gz"
env="WARDEN_DATA=/home/sprite/warden-data,WARDEN_ADMIN_TOKEN=$WARDEN_ADMIN_TOKEN,GITHUB_CONNECTION_ID=$GITHUB_CONNECTION_ID,PORT=8080,SPRITES_TOKEN=$SPRITES_TOKEN,AGENT_SPRITE=$AGENT_SPRITE,WARDEN_CHECKPOINT_ON_WRITE=1"
sprite exec --no-stdin -s "$WARDEN_SPRITE" -- bash -c "sprite-env services delete warden >/dev/null 2>&1 || true
mv ~/bin/mandate-warden.new ~/bin/mandate-warden
sprite-env services create warden --cmd /home/sprite/bin/mandate-warden --dir /home/sprite --env '$env' --http-port 8080 --no-stream >/dev/null
sleep 1; sprite-env services list | grep -o '\"status\":\"[a-z]*\"'"

echo "uploading warden-call to $AGENT_SPRITE"
sprite exec --stdin -s "$AGENT_SPRITE" -- bash -c 'mkdir -p ~/bin && gunzip -c > ~/bin/warden-call && chmod +x ~/bin/warden-call' < "$tmp/c.gz"
echo "done"
