# Lynora

Low-memory, Rust-based API workbench (Postman-like) for REST, GraphQL, and gRPC — desktop + web.

**Status:** milestone **0.1** in progress — `lynora-core` + Tauri desktop REST workbench.

## Docs

- Design: [`docs/superpowers/specs/2026-07-22-lynora-design.md`](docs/superpowers/specs/2026-07-22-lynora-design.md)
- 0.1 plan: [`docs/superpowers/plans/2026-07-22-lynora-0.1-core-rest.md`](docs/superpowers/plans/2026-07-22-lynora-0.1-core-rest.md)

## Develop

### Core library

```bash
cargo test -p lynora-core
```

### Desktop (Linux / macOS)

Prerequisites: [Tauri Linux deps](https://tauri.app/start/prerequisites/) (`webkit2gtk4.1-devel`, `librsvg2-devel`, etc.).

```bash
cd apps/lynora-desktop
npm install
npm run tauri dev
```

Collections default to `~/.config/lynora/collections` (Linux). Create a collection, set `{{baseUrl}}` in the `local` environment, then Send.

## License

MIT
