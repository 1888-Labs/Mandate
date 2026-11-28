# Contributing to Mandate

Thanks for wanting to help. Mandate is still young (built for Fly.io Hacksgiving 2026) and contributions of all sizes are welcome.

## Before you start

1. Read the [README](README.md) and the [security model](docs/security.md).
2. Skim the [protocol](docs/protocol.md) so the attenuation rules and deny codes make sense.
3. Check the [roadmap](docs/roadmap.md) to see what is already planned.

## Development setup

You need a recent Rust toolchain (1.80+ recommended).

```bash
git clone https://github.com/1888-Labs/Mandate.git
cd Mandate

# Format, lint, and test (these must pass before any change to authorization logic)
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

All three commands above are required for changes that touch `mandate-core` or `mandate-crypto`. Protocol rules are covered by tests; do not delete tests to make CI pass.

### Running the Warden locally

Without real GitHub/Ollama credentials the Warden still exercises signing, classification and authorization (allowed calls return `503 NO_CONNECTOR`):

```bash
WARDEN_DATA=./warden-data WARDEN_ADMIN_TOKEN=dev \
  cargo run -p mandate-warden --bin mandate-warden
```

In another terminal:

```bash
eval "$(cargo run -q -p mandate-warden --bin warden-call keygen | sed -n 1p | sed 's/^/export /')"
export MANDATE_WARDEN_URL=http://127.0.0.1:8080
AGENT=$(cargo run -q -p mandate-warden --bin warden-call address)

curl -s -X POST localhost:8080/v1/admin/mandates \
  -H 'authorization: Bearer dev' -H 'content-type: application/json' \
  -d "{\"subject\":\"$AGENT\",\"action\":\"read\",\"resource\":\"github:octo/demo\"}"

cargo run -q -p mandate-warden --bin warden-call GET repos/octo/demo/issues
```

See [docs/development.md](docs/development.md) and [docs/self-hosting.md](docs/self-hosting.md) for Docker and Sprites setups.

## What makes a good contribution

| Kind | Examples |
|------|----------|
| Documentation | Clarifications, missing examples, typo fixes, better diagrams |
| Tests | Extra edge cases for attenuation, signature verification, classifier paths |
| Hardening | Better error messages, input validation, admin-auth improvements |
| Features | New connectors, cross-call budgets, MCP proxy, signed receipts (see roadmap) |
| Packaging | crates.io publish, better Docker image tags, CI improvements |

Larger design changes (new constraint types, changes to the attenuation rules, new deny codes) should be discussed in an issue first so we keep the protocol stable.

## Pull request checklist

- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] New protocol behaviour has tests
- [ ] Docs updated if you changed user-visible behaviour
- [ ] Commit messages are short and imperative

## Reporting security issues

Please do **not** open a public issue for vulnerabilities. See [SECURITY.md](SECURITY.md).
