type Header = { key: string; value: string; enabled: boolean };
type RequestDoc = {
  id: string;
  name: string;
  method: string;
  url: string;
  headers: Header[];
  body?: string | null;
  protocol: "rest" | "graphql";
  graphql?: { query: string; variables?: string | null } | null;
};
type Collection = {
  id: string;
  name: string;
  requests: RequestDoc[];
  updatedAt: string;
};

const DB_NAME = "lynora-web";
const STORE = "collections";
const AUTH_KEY = "lynora-web-auth";

const els = {
  collectionList: document.querySelector("#collection-list") as HTMLUListElement,
  requestList: document.querySelector("#request-list") as HTMLUListElement,
  protocol: document.querySelector("#protocol") as HTMLSelectElement,
  method: document.querySelector("#method") as HTMLSelectElement,
  url: document.querySelector("#url") as HTMLInputElement,
  name: document.querySelector("#req-name") as HTMLInputElement,
  headers: document.querySelector("#headers") as HTMLTextAreaElement,
  body: document.querySelector("#body") as HTMLTextAreaElement,
  gqlQuery: document.querySelector("#gql-query") as HTMLTextAreaElement,
  gqlVars: document.querySelector("#gql-vars") as HTMLTextAreaElement,
  restPane: document.querySelector("#rest-pane") as HTMLDivElement,
  gqlPane: document.querySelector("#gql-pane") as HTMLDivElement,
  status: document.querySelector("#status") as HTMLSpanElement,
  duration: document.querySelector("#duration") as HTMLSpanElement,
  responseBody: document.querySelector("#response-body") as HTMLPreElement,
  syncUrl: document.querySelector("#sync-url") as HTMLInputElement,
  email: document.querySelector("#email") as HTMLInputElement,
  password: document.querySelector("#password") as HTMLInputElement,
  authStatus: document.querySelector("#auth-status") as HTMLSpanElement,
};

let collections: Collection[] = [];
let activeId: string | null = null;
let activeRequestId: string | null = null;
let authToken: string | null = localStorage.getItem(AUTH_KEY);

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 1);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(STORE)) db.createObjectStore(STORE, { keyPath: "id" });
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function loadAll(): Promise<Collection[]> {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE, "readonly");
    const req = tx.objectStore(STORE).getAll();
    req.onsuccess = () => resolve(req.result as Collection[]);
    req.onerror = () => reject(req.error);
  });
}

async function saveCollection(col: Collection) {
  const db = await openDb();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE, "readwrite");
    tx.objectStore(STORE).put(col);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

function uid() {
  return crypto.randomUUID();
}

function expand(input: string, vars: Record<string, string>) {
  return input.replace(/\{\{\s*([^}]+?)\s*\}\}/g, (_, key) => {
    if (!(key in vars)) throw new Error(`missing variable: ${key}`);
    return vars[key];
  });
}

function syncProtocolUi() {
  const gql = els.protocol.value === "graphql";
  els.gqlPane.classList.toggle("hidden", !gql);
  els.restPane.classList.toggle("hidden", gql);
  if (gql) els.method.value = "POST";
}

function activeCollection() {
  return collections.find((c) => c.id === activeId) ?? null;
}

function renderCollections() {
  els.collectionList.innerHTML = "";
  for (const col of collections) {
    const li = document.createElement("li");
    const btn = document.createElement("button");
    btn.textContent = col.name;
    btn.className = col.id === activeId ? "active" : "";
    btn.onclick = () => {
      activeId = col.id;
      activeRequestId = null;
      renderCollections();
      renderRequests();
      clearEditor();
    };
    li.appendChild(btn);
    els.collectionList.appendChild(li);
  }
}

function renderRequests() {
  els.requestList.innerHTML = "";
  const col = activeCollection();
  if (!col) return;
  for (const req of col.requests) {
    const li = document.createElement("li");
    const btn = document.createElement("button");
    btn.textContent = `${req.protocol === "graphql" ? "GQL" : req.method} ${req.name}`;
    btn.className = req.id === activeRequestId ? "active" : "";
    btn.onclick = () => loadRequest(req);
    li.appendChild(btn);
    els.requestList.appendChild(li);
  }
}

function clearEditor() {
  activeRequestId = null;
  els.name.value = "Untitled";
  els.protocol.value = "rest";
  els.method.value = "GET";
  els.url.value = "";
  els.headers.value = JSON.stringify(
    [{ key: "Accept", value: "application/json", enabled: true }],
    null,
    2,
  );
  els.body.value = "";
  els.gqlQuery.value = "query { __typename }";
  els.gqlVars.value = "{}";
  syncProtocolUi();
}

function loadRequest(req: RequestDoc) {
  activeRequestId = req.id;
  els.name.value = req.name;
  els.protocol.value = req.protocol;
  els.method.value = req.method;
  els.url.value = req.url;
  els.headers.value = JSON.stringify(req.headers, null, 2);
  els.body.value = req.body ?? "";
  els.gqlQuery.value = req.graphql?.query ?? "query { __typename }";
  els.gqlVars.value = req.graphql?.variables ?? "{}";
  syncProtocolUi();
  renderRequests();
}

function parseHeaders(): Header[] {
  const parsed = JSON.parse(els.headers.value || "[]");
  if (!Array.isArray(parsed)) throw new Error("headers must be an array");
  return parsed;
}

function currentRequest(): RequestDoc {
  const protocol = els.protocol.value as "rest" | "graphql";
  return {
    id: activeRequestId ?? uid(),
    name: els.name.value || "Untitled",
    method: protocol === "graphql" ? "POST" : els.method.value,
    url: els.url.value,
    headers: parseHeaders(),
    body: protocol === "graphql" ? els.gqlQuery.value : els.body.value || null,
    protocol,
    graphql:
      protocol === "graphql"
        ? { query: els.gqlQuery.value, variables: els.gqlVars.value || "{}" }
        : null,
  };
}

async function sendBrowser() {
  const vars = { baseUrl: "http://127.0.0.1:3000" };
  const req = currentRequest();
  const url = expand(req.url, vars);
  const headers = new Headers();
  for (const h of req.headers) {
    if (h.enabled) headers.set(h.key, expand(h.value, vars));
  }
  let body: string | undefined;
  if (req.protocol === "graphql") {
    headers.set("Content-Type", "application/json");
    body = JSON.stringify({
      query: expand(req.graphql?.query ?? "", vars),
      variables: JSON.parse(req.graphql?.variables || "{}"),
    });
  } else if (req.body) {
    body = expand(req.body, vars);
  }
  const started = performance.now();
  const resp = await fetch(url, { method: req.method, headers, body });
  const text = await resp.text();
  els.status.textContent = String(resp.status);
  els.duration.textContent = `${Math.round(performance.now() - started)} ms`;
  els.responseBody.textContent = text;
}

function updateAuthUi() {
  els.authStatus.textContent = authToken ? "Signed in" : "Local only";
}

async function auth(path: "login" | "register") {
  const base = els.syncUrl.value.replace(/\/$/, "");
  const resp = await fetch(`${base}/auth/${path === "login" ? "login" : "register"}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ email: els.email.value, password: els.password.value }),
  });
  const text = await resp.text();
  if (!resp.ok) throw new Error(text || resp.statusText);
  const data = JSON.parse(text) as { token: string };
  authToken = data.token;
  localStorage.setItem(AUTH_KEY, authToken);
  updateAuthUi();
}

async function syncNow() {
  if (!authToken) throw new Error("Sign in first");
  const base = els.syncUrl.value.replace(/\/$/, "");
  // Push all local collections as bundles
  for (const col of collections) {
    const bundle = {
      meta: { id: col.id, name: col.name, version: 1 },
      requests: col.requests.map((r) => ({
        ...r,
        auth: null,
        grpc: null,
      })),
      environments: [],
      updatedAt: new Date().toISOString(),
      contentHash: await sha256(JSON.stringify(col.requests)),
    };
    const resp = await fetch(`${base}/sync/collections/${col.id}?force=true`, {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${authToken}`,
      },
      body: JSON.stringify(bundle),
    });
    if (!resp.ok) throw new Error(await resp.text());
  }
  // Pull remote list and merge missing
  const listResp = await fetch(`${base}/sync/collections`, {
    headers: { Authorization: `Bearer ${authToken}` },
  });
  if (!listResp.ok) throw new Error(await listResp.text());
  const remote = (await listResp.json()) as { id: string; name: string }[];
  for (const item of remote) {
    if (collections.some((c) => c.id === item.id)) continue;
    const getResp = await fetch(`${base}/sync/collections/${item.id}`, {
      headers: { Authorization: `Bearer ${authToken}` },
    });
    if (!getResp.ok) continue;
    const bundle = await getResp.json();
    const col: Collection = {
      id: bundle.meta.id,
      name: bundle.meta.name,
      requests: bundle.requests ?? [],
      updatedAt: bundle.updatedAt,
    };
    collections.push(col);
    await saveCollection(col);
  }
  renderCollections();
  els.responseBody.textContent = "Sync complete.";
}

async function sha256(text: string) {
  const data = new TextEncoder().encode(text);
  const hash = await crypto.subtle.digest("SHA-256", data);
  return [...new Uint8Array(hash)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

els.protocol.addEventListener("change", syncProtocolUi);

document.querySelector("#btn-new-collection")!.addEventListener("click", async () => {
  const name = prompt("Collection name", "My API");
  if (!name) return;
  const col: Collection = {
    id: uid(),
    name,
    requests: [],
    updatedAt: new Date().toISOString(),
  };
  collections.push(col);
  await saveCollection(col);
  activeId = col.id;
  renderCollections();
  renderRequests();
  clearEditor();
});

document.querySelector("#btn-new-request")!.addEventListener("click", () => {
  if (!activeCollection()) {
    alert("Create a collection first");
    return;
  }
  clearEditor();
});

document.querySelector("#btn-save")!.addEventListener("click", async () => {
  const col = activeCollection();
  if (!col) return alert("Create a collection first");
  const req = currentRequest();
  const idx = col.requests.findIndex((r) => r.id === req.id);
  if (idx >= 0) col.requests[idx] = req;
  else col.requests.push(req);
  col.updatedAt = new Date().toISOString();
  activeRequestId = req.id;
  await saveCollection(col);
  renderRequests();
});

document.querySelector("#btn-send")!.addEventListener("click", async () => {
  try {
    await sendBrowser();
  } catch (e) {
    els.status.textContent = "ERR";
    els.responseBody.textContent = String(e);
  }
});

document.querySelector("#btn-login")!.addEventListener("click", async () => {
  try {
    await auth("login");
  } catch (e) {
    alert(String(e));
  }
});
document.querySelector("#btn-register")!.addEventListener("click", async () => {
  try {
    await auth("register");
  } catch (e) {
    alert(String(e));
  }
});
document.querySelector("#btn-sync")!.addEventListener("click", async () => {
  try {
    await syncNow();
  } catch (e) {
    alert(String(e));
  }
});

async function boot() {
  collections = await loadAll();
  updateAuthUi();
  syncProtocolUi();
  if (collections[0]) {
    activeId = collections[0].id;
  }
  renderCollections();
  renderRequests();
  clearEditor();
}

boot().catch((e) => {
  els.responseBody.textContent = `Failed to start: ${e}`;
});
