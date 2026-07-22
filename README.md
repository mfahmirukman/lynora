# Lynora

Low-memory, Rust-based API workbench (Postman-like) for REST, GraphQL, and gRPC — desktop + web.

**Status:** milestones **0.1–0.3** landed — core + desktop with REST/GraphQL/gRPC, auth, codegen, Postman/OpenAPI/proto import.

## Docs

- Design: [`docs/superpowers/specs/2026-07-22-lynora-design.md`](docs/superpowers/specs/2026-07-22-lynora-design.md)
- Plans: [`docs/superpowers/plans/`](docs/superpowers/plans/)

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

Collections default to `~/.config/lynora/collections` (Linux).

## License

MIT
