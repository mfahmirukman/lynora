# Lynora

Low-memory, Rust-based API workbench (Postman-like) for REST, GraphQL, and gRPC — desktop + web.

**Status:** milestones **0.1–0.4** — core, desktop, web, optional sync, Linux/macOS bundle targets.

## Docs

- Design: [`docs/superpowers/specs/2026-07-22-lynora-design.md`](docs/superpowers/specs/2026-07-22-lynora-design.md)
- Plans: [`docs/superpowers/plans/`](docs/superpowers/plans/)

## Develop

### Core / sync tests

```bash
cargo test -p lynora-core
cargo test -p lynora-sync
```

### Sync server (optional)

```bash
cargo run -p lynora-sync-server
# listens on http://0.0.0.0:8787
```

### Desktop (Linux / macOS)

```bash
cd apps/lynora-desktop
npm install
npm run tauri dev
```

### Web

```bash
cd apps/lynora-web
npm install
npm run dev
# http://localhost:5173
```

Browser supports REST & GraphQL; gRPC remains desktop-only in 0.4.

### Installers

```bash
cd apps/lynora-desktop
npm run tauri build
```

Configured bundle targets: **deb**, **AppImage** (Linux), **dmg** / **app** (macOS).

## License

MIT
