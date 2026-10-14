# Development

[← Back to the README](../README.md)

## Rust

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

All three must pass before any change to authorization logic is merged. Every protocol rule has a test, and tests are never deleted to make CI pass.

## Arkiv adapter

```bash
cd apps/arkiv-adapter
npm install
npm run typecheck
npm test                    # offline tests
npm run test:arkiv          # on-chain acceptance on Tiramisu (needs a funded ARKIV_PRIVATE_KEY in .env)
npm run test:arkiv-expiry   # entity expiry on Tiramisu (~4 minutes)
```

## Web UI

React + TypeScript + Vite, in [`apps/web`](../apps/web). `mandate-server` serves the built `apps/web/dist` (`WEB_DIR` to change it).

```bash
cd apps/web
npm install
npm run dev        # http://127.0.0.1:5173, proxying /v1 to mandate-server on :8080 (start it first: scripts/run-local.sh)
npm run build      # type-check and build into dist/
npm test           # unit tests (vitest)
```

## Docs site

Built with [Vocs](https://vocs.dev), the framework behind viem.sh, in [`apps/docs`](../apps/docs). The pages come from this repository's `docs/` and `arkiv/` markdown: `npm run sync` copies them in and rewrites their links, so these files stay the single source of truth.

```bash
cd apps/docs
npm install
npm run dev        # sync, then a live-reloading docs server
npm run build      # sync, then a static site in dist/public
```

## End to end

```bash
scripts/demo-arkiv.sh             # CLI → mandate-server → adapter → Arkiv, all 7 demo steps, checked
scripts/seed-arkiv.sh             # (re)publish the long-lived demo dataset to the main namespace
scripts/run-local.sh              # the web app against the live dataset → http://127.0.0.1:8080
cargo run -p finance-agent        # the prompt-injected agent demo, in memory
```

## Roadmap

- [x] **M1** Core protocol types
- [x] **M2** Canonical encoding and deterministic mandate IDs
- [x] **M3** Delegation validation (`can_delegate`)
- [x] **M4** Ethereum-compatible signatures
- [x] **M5** Authorization engine: requests, decisions, evidence, chain traversal, expiry, trust anchors
- [x] **M6** Revocation and ancestor revocation
- [x] **M7** In-memory store and the full demo flow without Arkiv
- [x] **M8** CLI
- [x] **M9** Arkiv adapter (TypeScript, Tiramisu testnet), [`arkiv/schema.md`](../arkiv/schema.md), [`arkiv/friction.md`](../arkiv/friction.md)
- [x] **M10** Rust HTTP server, backed by Arkiv
- [x] **M11** Demo agent: a prompt-injected agent blocked by Mandate ([`examples/finance-agent`](../examples/finance-agent))
- [x] **M12** Web UI: delegation graph, authorization tester, live revocation sandbox, live Arkiv events
- [ ] Public deployment, demo video

Out of scope for v0.1: tokens, NFTs, DAOs, a custom policy language, DIDs, custom chains, ZK proofs, multisig, production money movement.
