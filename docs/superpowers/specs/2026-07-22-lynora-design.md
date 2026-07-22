# Lynora Design Spec

**Date:** 2026-07-22  
**Status:** Approved for planning  
**Product:** Lynora — a low-memory, Rust-based API workbench (Postman-like)

## 1. Purpose & audience

Lynora is a **personal developer toolkit** for exploring and exercising APIs. Primary goals:

- Native desktop app with **low memory** use relative to Electron-based clients
- Support **REST, GraphQL, and gRPC**
- **Desktop + web** in the first major release
- **Optional** sign-in with collection sync; fully usable offline without an account
- **Import** Postman collections via JSON
- Collections stored as **git-friendly files on disk**

**Out of scope for 1.0:** team workspaces, billing, pre/post scripts & tests, local mock server, Windows packaging.

## 2. Goals & non-goals

### Goals (1.0)

- Send and inspect REST, GraphQL, and gRPC requests from desktop (Linux + macOS) and a basic web client
- Environments/variables with masked secrets
- Request history and replay
- Auth helpers (Bearer, Basic, API key, OAuth2 PKCE, AWS SigV4)
- Code generation (curl, fetch, reqwest, and similar)
- Import: Postman JSON, OpenAPI, Protobuf (`.proto`)
- Optional cloud sync for collections when signed in
- CLI / headless collection runner for CI
- WebSockets and SSE

### Non-goals (1.0)

- Feature parity with Postman/Insomnia enterprise collaboration
- Guaranteeing the lowest possible RAM vs pure egui (Tauri webview is accepted for UX speed)
- Full gRPC reflection / every streaming edge case in 0.3 (basics first, polish by 1.0)
- Scripts, test assertions, and mock servers (post-1.0)

## 3. Architecture

**Approach:** core-first Cargo workspace with thin clients.

```
Clients:  lynora-desktop (Tauri 2) · lynora-web · lynora-cli
                │                      │            │
                └──────────────────────┴────────────┘
                                   │
                            lynora-core
                     (protocols, collections, env,
                      import, history, auth, codegen)
                                   │ optional
                            lynora-sync
                     (auth, collection sync)
```

| Crate / app | Role |
|-------------|------|
| `lynora-core` | Shared Rust logic: protocol clients, collection I/O, environments, history, imports, auth helpers, codegen |
| `lynora-cli` | Headless runner; exit codes and machine-readable reports for CI |
| `lynora-desktop` | Tauri 2 shell hosting the shared web UI; OS keychain; native networking |
| `lynora-web` | Same UI in the browser; WASM and/or thin backend where browser/WASM limits apply (e.g. some gRPC) |
| `lynora-sync` | Optional account + sync service client; core works without it |

**UI choice:** Tauri 2 + shared web frontend (one UI for desktop webview and browser).

**Source of truth:** collection files on disk. Sync mirrors files; it does not replace the local file model.

## 4. Data model & flow

### On disk

- **Collections:** user-chosen directory; one folder per collection; small JSON (or similar) files per request — diffable in git
- **Environments:** separate profiles under config; secrets in OS keychain / encrypted store, not committed
- **History:** local index (sqlite or append-only files); searchable; replayable; sync of history is not required in 1.0
- **Settings:** local config (`settings.toml` or equivalent)

### Send path

1. UI loads request + active environment  
2. Expand `{{variables}}`  
3. Apply auth helper  
4. Dispatch via protocol client (REST / GraphQL / gRPC / later WS & SSE)  
5. Render response  
6. Append history  
7. Optionally enqueue sync delta if signed in  

### Sync (from 0.4)

- Optional sign-in  
- Sync collection and non-secret environment metadata  
- Never sync plaintext secrets  
- Offline-first: desktop fully usable without account  

## 5. Error handling

| Case | Behavior |
|------|----------|
| Transport (DNS, TLS, timeout) | Surface in response pane; keep request editable; no UI crash |
| HTTP / GraphQL / gRPC application errors | Treat as successful round-trips with status/error panels |
| Import failures | Partial import + report of skipped items; never wipe existing data on failure |
| Missing vars / auth | Fail before send with a clear message |
| Sync conflicts | Last-write-wins + local backup snapshot; user-visible notice; no silent body merges in 1.0 |
| Secrets | Never log or sync plaintext; redacted in exports |

## 6. Feature roadmap (extras)

Included toward 1.0 (in addition to core protocols and Postman import):

1. Environments & variables  
2. Request history & replay  
3. Auth helpers  
4. Code generation  
5. OpenAPI / Protobuf import  
6. CLI / headless runner  
7. WebSockets & SSE  
8. Git-friendly collections  

Deferred: pre/post scripts & tests, local mock server.

## 7. Version milestones

| Version | Description | Milestone outcomes |
|---------|-------------|-------------------|
| **0.1** | Core + REST workbench | `lynora-core` + Tauri desktop; collections on disk; environments; send REST; Postman JSON import; local history |
| **0.2** | Auth + GraphQL + codegen | Auth helpers; GraphQL editor/introspection basics; export curl / fetch / reqwest |
| **0.3** | gRPC + schema import | Unary + basic streaming gRPC; OpenAPI and `.proto` → requests |
| **0.4** | Web + optional sync | Browser build; optional sign-in; collection sync; Linux + macOS installers |
| **0.5** | CLI + realtime | `lynora-cli` CI runner; WebSockets & SSE |
| **1.0** | First major release | Polish, docs, stability across REST/GraphQL/gRPC, desktop+web, optional sync, imports, CLI |

**Platforms for 1.0:** Linux and macOS. Windows later.

**Memory:** document an informal idle RSS target on Linux vs typical Electron clients; formal CI memory gates are optional later.

## 8. Testing strategy

- **Unit tests** in `lynora-core`: variable expansion, auth headers, parsers (Postman/OpenAPI/proto), collection round-trips  
- **Integration tests:** local HTTP, GraphQL, and gRPC test servers in CI (no live internet)  
- **CLI golden tests:** fixture collections → exit codes and report shape  
- **Desktop/web:** smoke tests (boot + mock send); deep UI automation is secondary  
- **Sync:** contract tests against a mock sync API, including conflict/backup behavior  

## 9. Success criteria for 1.0

- Personal daily-driver usable for REST, GraphQL, and gRPC on Linux/macOS desktop  
- Basic web client with optional sync; local-only mode still works  
- Postman JSON import of a typical personal collection succeeds with a clear report  
- Collections remain editable/diffable in git  
- CLI can run a collection in CI and fail the run on transport errors and simple expected-status checks (no scripting engine in 1.0)  
- Noticeably lighter idle memory than mainstream Electron API clients on a reference Linux machine (qualitative + documented numbers)

## 10. Open decisions deferred to implementation planning

- Exact web stack inside Tauri (e.g. React vs Svelte vs solid)  
- Collection file schema details (single manifest vs one-file-per-request naming)  
- Sync backend hosting (self-hosted vs managed) for a personal toolkit  
- Precise gRPC streaming UI and WASM fallback strategy for browser  

These do not block the product shape above; they belong in the implementation plan.
