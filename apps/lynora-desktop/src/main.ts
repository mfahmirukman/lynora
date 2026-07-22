import { invoke } from "@tauri-apps/api/core";

type Header = { key: string; value: string; enabled: boolean };
type Protocol = "rest" | "graphql";
type AuthKind = "none" | "bearer" | "basic" | "apiKey" | "oauth2Pkce" | "awsSigV4";

type AuthConfig = {
  kind: AuthKind;
  token?: string | null;
  username?: string | null;
  password?: string | null;
  keyName?: string | null;
  apiKeyIn?: "header" | "query";
  accessKeyId?: string | null;
  secretAccessKey?: string | null;
  region?: string | null;
  service?: string | null;
  clientId?: string | null;
};

type GraphQlBody = {
  query: string;
  variables?: string | null;
  operationName?: string | null;
};

type CollectionSummary = { path: string; id: string; name: string };

type RequestDocument = {
  id: string;
  name: string;
  method: string;
  url: string;
  headers: Header[];
  body?: string | null;
  protocol?: Protocol;
  auth?: AuthConfig | null;
  graphql?: GraphQlBody | null;
};

type CollectionDto = {
  path: string;
  id: string;
  name: string;
  requests: RequestDocument[];
};

type Environment = {
  name: string;
  values: Record<string, string>;
  secrets: string[];
};

type RestResponse = {
  status: number;
  headers: [string, string][];
  body: string;
  duration_ms: number;
};

const els = {
  collectionList: document.querySelector("#collection-list") as HTMLUListElement,
  requestList: document.querySelector("#request-list") as HTMLUListElement,
  envSelect: document.querySelector("#env-select") as HTMLSelectElement,
  protocol: document.querySelector("#protocol") as HTMLSelectElement,
  method: document.querySelector("#method") as HTMLSelectElement,
  url: document.querySelector("#url") as HTMLInputElement,
  name: document.querySelector("#req-name") as HTMLInputElement,
  headers: document.querySelector("#headers") as HTMLTextAreaElement,
  body: document.querySelector("#body") as HTMLTextAreaElement,
  gqlQuery: document.querySelector("#gql-query") as HTMLTextAreaElement,
  gqlVars: document.querySelector("#gql-vars") as HTMLTextAreaElement,
  authKind: document.querySelector("#auth-kind") as HTMLSelectElement,
  authToken: document.querySelector("#auth-token") as HTMLInputElement,
  authUsername: document.querySelector("#auth-username") as HTMLInputElement,
  authExtra: document.querySelector("#auth-extra") as HTMLInputElement,
  exportLang: document.querySelector("#export-lang") as HTMLSelectElement,
  restPane: document.querySelector("#rest-body-pane") as HTMLDivElement,
  gqlPane: document.querySelector("#gql-pane") as HTMLDivElement,
  btnIntrospect: document.querySelector("#btn-introspect") as HTMLButtonElement,
  status: document.querySelector("#status") as HTMLSpanElement,
  duration: document.querySelector("#duration") as HTMLSpanElement,
  responseBody: document.querySelector("#response-body") as HTMLPreElement,
  importFile: document.querySelector("#import-file") as HTMLInputElement,
};

let collections: CollectionSummary[] = [];
let activeCollection: CollectionDto | null = null;
let activeRequestId: string | null = null;

function syncProtocolUi() {
  const gql = els.protocol.value === "graphql";
  els.gqlPane.classList.toggle("hidden", !gql);
  els.restPane.classList.toggle("hidden", gql);
  els.btnIntrospect.classList.toggle("hidden", !gql);
  if (gql) els.method.value = "POST";
}

function syncAuthUi() {
  const kind = els.authKind.value as AuthKind;
  const needsUser = kind === "basic" || kind === "awsSigV4" || kind === "oauth2Pkce";
  const needsExtra =
    kind === "apiKey" || kind === "awsSigV4" || kind === "basic" || kind === "oauth2Pkce";
  els.authUsername.classList.toggle("hidden", !needsUser);
  els.authExtra.classList.toggle("hidden", !needsExtra);
  els.authToken.classList.toggle("hidden", kind === "none");
}

function readAuth(): AuthConfig | null {
  const kind = els.authKind.value as AuthKind;
  if (kind === "none") return null;
  const cfg: AuthConfig = { kind };
  if (kind === "bearer" || kind === "apiKey" || kind === "oauth2Pkce") {
    cfg.token = els.authToken.value || null;
  }
  if (kind === "basic") {
    cfg.username = els.authUsername.value || null;
    cfg.password = els.authToken.value || null;
  }
  if (kind === "apiKey") {
    cfg.keyName = els.authExtra.value || "X-API-Key";
    cfg.apiKeyIn = "header";
  }
  if (kind === "awsSigV4") {
    cfg.accessKeyId = els.authUsername.value || null;
    cfg.secretAccessKey = els.authToken.value || null;
    cfg.region = els.authExtra.value || "us-east-1";
    cfg.service = "execute-api";
  }
  if (kind === "oauth2Pkce") {
    cfg.clientId = els.authUsername.value || null;
  }
  return cfg;
}

function writeAuth(auth?: AuthConfig | null) {
  if (!auth) {
    els.authKind.value = "none";
    els.authToken.value = "";
    els.authUsername.value = "";
    els.authExtra.value = "";
    syncAuthUi();
    return;
  }
  els.authKind.value = auth.kind;
  if (auth.kind === "basic") {
    els.authUsername.value = auth.username ?? "";
    els.authToken.value = auth.password ?? "";
  } else if (auth.kind === "awsSigV4") {
    els.authUsername.value = auth.accessKeyId ?? "";
    els.authToken.value = auth.secretAccessKey ?? "";
    els.authExtra.value = auth.region ?? "us-east-1";
  } else if (auth.kind === "apiKey") {
    els.authToken.value = auth.token ?? "";
    els.authExtra.value = auth.keyName ?? "X-API-Key";
  } else {
    els.authToken.value = auth.token ?? "";
    els.authUsername.value = auth.clientId ?? auth.username ?? "";
  }
  syncAuthUi();
}

async function refreshCollections() {
  collections = await invoke<CollectionSummary[]>("list_collections");
  els.collectionList.innerHTML = "";
  for (const col of collections) {
    const li = document.createElement("li");
    const btn = document.createElement("button");
    btn.textContent = col.name;
    btn.className = activeCollection?.path === col.path ? "active" : "";
    btn.onclick = () => void openCollection(col.path);
    li.appendChild(btn);
    els.collectionList.appendChild(li);
  }
}

async function refreshEnvironments() {
  const envs = await invoke<Environment[]>("list_environments");
  const current = els.envSelect.value;
  els.envSelect.innerHTML = `<option value="">No environment</option>`;
  for (const env of envs) {
    const opt = document.createElement("option");
    opt.value = env.name;
    opt.textContent = env.name;
    els.envSelect.appendChild(opt);
  }
  if ([...els.envSelect.options].some((o) => o.value === current)) {
    els.envSelect.value = current;
  }
}

async function openCollection(path: string) {
  activeCollection = await invoke<CollectionDto>("load_collection", { path });
  activeRequestId = null;
  renderRequests();
  await refreshCollections();
  clearEditor();
}

function renderRequests() {
  els.requestList.innerHTML = "";
  if (!activeCollection) return;
  for (const req of activeCollection.requests) {
    const li = document.createElement("li");
    const btn = document.createElement("button");
    const proto = req.protocol === "graphql" ? "GQL" : req.method;
    btn.textContent = `${proto} ${req.name}`;
    btn.className = activeRequestId === req.id ? "active" : "";
    btn.onclick = () => loadRequestIntoEditor(req);
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
  writeAuth(null);
  els.status.textContent = "—";
  els.duration.textContent = "";
  els.responseBody.textContent = "Send a request to see the response.";
  syncProtocolUi();
}

function loadRequestIntoEditor(req: RequestDocument) {
  activeRequestId = req.id;
  els.name.value = req.name;
  els.protocol.value = req.protocol === "graphql" ? "graphql" : "rest";
  els.method.value = req.method;
  els.url.value = req.url;
  els.headers.value = JSON.stringify(req.headers, null, 2);
  els.body.value = req.body ?? "";
  els.gqlQuery.value = req.graphql?.query ?? req.body ?? "query { __typename }";
  els.gqlVars.value = req.graphql?.variables ?? "{}";
  writeAuth(req.auth);
  syncProtocolUi();
  renderRequests();
}

function parseHeaders(): Header[] {
  const parsed = JSON.parse(els.headers.value || "[]");
  if (!Array.isArray(parsed)) throw new Error("headers must be an array");
  return parsed;
}

function currentGraphql(): GraphQlBody | null {
  if (els.protocol.value !== "graphql") return null;
  return {
    query: els.gqlQuery.value,
    variables: els.gqlVars.value || "{}",
    operationName: null,
  };
}

function requestPayload() {
  const protocol = els.protocol.value as Protocol;
  return {
    method: protocol === "graphql" ? "POST" : els.method.value,
    url: els.url.value,
    headers: parseHeaders(),
    body: protocol === "graphql" ? els.gqlQuery.value : els.body.value || null,
    environmentName: els.envSelect.value || null,
    protocol,
    auth: readAuth(),
    graphql: currentGraphql(),
  };
}

async function ensureDefaultEnvironment() {
  const envs = await invoke<Environment[]>("list_environments");
  if (envs.length === 0) {
    await invoke("save_environment", {
      env: {
        name: "local",
        values: { baseUrl: "http://127.0.0.1:3000" },
        secrets: [],
      },
    });
  }
}

els.protocol.addEventListener("change", syncProtocolUi);
els.authKind.addEventListener("change", syncAuthUi);

document.querySelector("#btn-new-collection")!.addEventListener("click", async () => {
  const name = prompt("Collection name", "My API");
  if (!name) return;
  const created = await invoke<CollectionSummary>("create_collection", { name });
  await refreshCollections();
  await openCollection(created.path);
});

document.querySelector("#btn-new-request")!.addEventListener("click", () => {
  if (!activeCollection) {
    alert("Create or open a collection first.");
    return;
  }
  clearEditor();
  renderRequests();
});

document.querySelector("#btn-save")!.addEventListener("click", async () => {
  if (!activeCollection) {
    alert("Create or open a collection first.");
    return;
  }
  try {
    const base = requestPayload();
    const saved = await invoke<RequestDocument>("save_request", {
      input: {
        collectionPath: activeCollection.path,
        id: activeRequestId,
        name: els.name.value || "Untitled",
        ...base,
      },
    });
    activeRequestId = saved.id;
    activeCollection = await invoke<CollectionDto>("load_collection", {
      path: activeCollection.path,
    });
    renderRequests();
  } catch (e) {
    alert(String(e));
  }
});

document.querySelector("#btn-send")!.addEventListener("click", async () => {
  try {
    const response = await invoke<RestResponse>("send_request", {
      input: requestPayload(),
    });
    els.status.textContent = String(response.status);
    els.duration.textContent = `${response.duration_ms} ms`;
    els.responseBody.textContent = response.body;
  } catch (e) {
    els.status.textContent = "ERR";
    els.duration.textContent = "";
    els.responseBody.textContent = String(e);
  }
});

document.querySelector("#btn-export")!.addEventListener("click", async () => {
  try {
    const snippet = await invoke<string>("generate_snippet", {
      input: {
        language: els.exportLang.value,
        ...requestPayload(),
      },
    });
    await navigator.clipboard.writeText(snippet);
    els.responseBody.textContent = snippet;
    els.status.textContent = "SNIP";
  } catch (e) {
    alert(String(e));
  }
});

els.btnIntrospect.addEventListener("click", async () => {
  try {
    const response = await invoke<RestResponse>("introspect", {
      url: els.url.value,
      headers: parseHeaders(),
      environmentName: els.envSelect.value || null,
      auth: readAuth(),
    });
    els.status.textContent = String(response.status);
    els.duration.textContent = `${response.duration_ms} ms`;
    els.responseBody.textContent = response.body;
  } catch (e) {
    els.status.textContent = "ERR";
    els.responseBody.textContent = String(e);
  }
});

document.querySelector("#btn-import")!.addEventListener("click", () => {
  els.importFile.click();
});

els.importFile.addEventListener("change", async () => {
  const file = els.importFile.files?.[0];
  if (!file) return;
  const json = await file.text();
  try {
    const created = await invoke<CollectionSummary>("import_postman", { json });
    await refreshCollections();
    await openCollection(created.path);
  } catch (e) {
    alert(String(e));
  } finally {
    els.importFile.value = "";
  }
});

async function boot() {
  await ensureDefaultEnvironment();
  await refreshEnvironments();
  await refreshCollections();
  syncProtocolUi();
  syncAuthUi();
  if (collections[0]) {
    await openCollection(collections[0].path);
  } else {
    clearEditor();
  }
}

boot().catch((e) => {
  els.responseBody.textContent = `Failed to start: ${e}`;
});
