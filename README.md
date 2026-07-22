# Lynora

Low-memory, Rust-based API workbench (Postman-like) for REST, GraphQL, gRPC, WebSocket, and SSE — desktop + web.

**Status:** milestones **0.1–0.5** landed. Next: **1.0** polish.

## Docs

- Design: [`docs/superpowers/specs/2026-07-22-lynora-design.md`](docs/superpowers/specs/2026-07-22-lynora-design.md)
- Plans: [`docs/superpowers/plans/`](docs/superpowers/plans/)

## Develop

```bash
cargo test -p lynora-core -p lynora-sync -p lynora-cli

# CLI — run a collection in CI
cargo run -p lynora-cli -- run ./path/to/collection --env-file ./local.json --format json

# Sync server
cargo run -p lynora-sync-server

# Desktop
cd apps/lynora-desktop && npm install && npm run tauri dev

# Web
cd apps/lynora-web && npm install && npm run dev
```

### Installers

```bash
cd apps/lynora-desktop && npm run tauri build
```

Targets: deb, AppImage, dmg/app.

## License

MIT
