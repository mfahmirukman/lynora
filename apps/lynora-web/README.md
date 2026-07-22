# Lynora Web

Browser workbench for REST and GraphQL with IndexedDB storage and optional sync.

```bash
# Terminal 1 — sync server (optional)
cargo run -p lynora-sync-server

# Terminal 2 — web UI
cd apps/lynora-web
npm install
npm run dev
```

Open http://localhost:5173

gRPC remains desktop-only in 0.4 (browser/WASM limits).
