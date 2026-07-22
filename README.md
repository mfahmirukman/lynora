# Lynora

Low-memory, Rust-based API workbench (Postman-like) for REST, GraphQL, and gRPC — desktop + web.

**Status:** early development (milestone **0.1** — core + REST workbench).

## Docs

- Design: [`docs/superpowers/specs/2026-07-22-lynora-design.md`](docs/superpowers/specs/2026-07-22-lynora-design.md)
- 0.1 plan: [`docs/superpowers/plans/2026-07-22-lynora-0.1-core-rest.md`](docs/superpowers/plans/2026-07-22-lynora-0.1-core-rest.md)

## Develop

```bash
# Core library tests
cargo test -p lynora-core

# Desktop app (after Task 9 scaffold)
cd apps/lynora-desktop
npm install
npm run tauri dev
```

## License

MIT
