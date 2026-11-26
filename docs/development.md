# Development

[← Back to the README](../README.md)

## Rust

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

All three must pass before any change to authorization logic is merged. Every protocol rule has a test, and tests are never deleted to make CI pass.

## Running the Warden locally

The Warden runs anywhere. Without connector IDs, allowed calls return `503 NO_CONNECTOR`, which is enough to test signing, classification and decisions.

```bash
WARDEN_DATA=./warden-data WARDEN_ADMIN_TOKEN=dev cargo run -p mandate-warden --bin mandate-warden
```

In another shell:

```bash
eval "$(cargo run -q -p mandate-warden --bin warden-call keygen | sed -n 1p | sed 's/^/export /')"
export MANDATE_WARDEN_URL=http://127.0.0.1:8080
AGENT=$(cargo run -q -p mandate-warden --bin warden-call address)

curl -s -X POST localhost:8080/v1/admin/mandates -H 'authorization: Bearer dev' \
  -H 'content-type: application/json' \
  -d "{\"subject\":\"$AGENT\",\"action\":\"read\",\"resource\":\"github:octo/demo\"}"

cargo run -q -p mandate-warden --bin warden-call GET repos/octo/demo/issues      # allowed → 503 NO_CONNECTOR locally
cargo run -q -p mandate-warden --bin warden-call POST repos/octo/demo/issues '{}' # 403 NO_MANDATE
curl -s localhost:8080/v1/state
```

## Deploying to Sprites

```bash
scripts/deploy-warden.sh
```

This reads `.env.sprites` and deploys to the `mandate-warden` and `mandate-agent` Sprites (override the names with `WARDEN_SPRITE` and `AGENT_SPRITE`). For first-time setup, see [Setting up two Sprites](warden.md#setting-up-two-sprites).
