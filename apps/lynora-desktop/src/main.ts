import { invoke } from "@tauri-apps/api/core";

type Header = { key: string; value: string; enabled: boolean };
type Protocol = "rest" | "graphql" | "grpc";

type GrpcBody = {
  service: string;
  method: string;
  messageJson: string;
  protoFile?: string | null;
  streaming?: boolean;
  inputType?: string;
};
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
  grpc?: GrpcBody | null;
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
  grpcPane: document.querySelector("#grpc-pane") as HTMLDivElement,
  grpcService: document.querySelector("#grpc-service") as HTMLInputElement,
  grpcMethod: document.querySelector("#grpc-method") as HTMLInputElement,
  grpcMessage: document.querySelector("#grpc-message") as HTMLTextAreaElement,
  btnIntrospect: document.querySelector("#btn-introspect") as HTMLButtonElement,
  status: document.querySelector("#status") as HTMLSpanElement,
  duration: document.querySelector("#duration") as HTMLSpanElement,
  responseBody: document.querySelector("#response-body") as HTMLPreElement,
  importFile: document.querySelector("#import-file") as HTMLInputElement,
  importOpenapiFile: document.querySelector("#import-openapi-file") as HTMLInputElement,
  importProtoFile: document.querySelector("#import-proto-file") as HTMLInputElement,
};

let collections: CollectionSummary[] = [];
let activeCollection: CollectionDto | null = null;
let activeRequestId: string | null = null;

function syncProtocolUi() {
  const mode = els.protocol.value as Protocol;
  els.gqlPane.classList.toggle("hidden", mode !== "graphql");
  els.grpcPane.classList.toggle("hidden", mode !== "grpc");
  els.restPane.classList.toggle("hidden", mode !== "rest");
  els.btnIntrospect.classList.toggle("hidden", mode !== "graphql");
  if (mode === "graphql" || mode === "grpc") els.method.value = "POST";
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
    const proto =
      req.protocol === "graphql" ? "GQL" : req.protocol === "grpc" ? "gRPC" : req.method;
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
  els.grpcService.value = "";
  els.grpcMethod.value = "";
  els.grpcMessage.value = '{"name":"world"}';
  writeAuth(null);
  els.status.textContent = "—";
  els.duration.textContent = "";
  els.responseBody.textContent = "Send a request to see the response.";
  syncProtocolUi();
}

function loadRequestIntoEditor(req: RequestDocument) {
  activeRequestId = req.id;
  els.name.value = req.name;
  els.protocol.value =
    req.protocol === "graphql" ? "graphql" : req.protocol === "grpc" ? "grpc" : "rest";
  els.method.value = req.method;
  els.url.value = req.url;
  els.headers.value = JSON.stringify(req.headers, null, 2);
  els.body.value = req.body ?? "";
  els.gqlQuery.value = req.graphql?.query ?? req.body ?? "query { __typename }";
  els.gqlVars.value = req.graphql?.variables ?? "{}";
  els.grpcService.value = req.grpc?.service ?? "";
  els.grpcMethod.value = req.grpc?.method ?? "";
  els.grpcMessage.value = req.grpc?.messageJson ?? req.body ?? "{}";
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

function currentGrpc(): GrpcBody | null {
  if (els.protocol.value !== "grpc") return null;
  return {
    service: els.grpcService.value,
    method: els.grpcMethod.value,
    messageJson: els.grpcMessage.value || "{}",
    protoFile: "source.proto",
    streaming: false,
    inputType: "",
  };
}

function requestPayload() {
  const protocol = els.protocol.value as Protocol;
  return {
    method: protocol === "rest" ? els.method.value : "POST",
    url: els.url.value,
    headers: parseHeaders(),
    body:
      protocol === "graphql"
        ? els.gqlQuery.value
        : protocol === "grpc"
          ? els.grpcMessage.value
          : els.body.value || null,
    environmentName: els.envSelect.value || null,
    protocol,
    auth: readAuth(),
    graphql: currentGraphql(),
    grpc: currentGrpc(),
    collectionPath: activeCollection?.path ?? null,
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
        id: activeRequestId,
        name: els.name.value || "Untitled",
        ...base,
        collectionPath: activeCollection.path,
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
document.querySelector("#btn-import-openapi")!.addEventListener("click", () => {
  els.importOpenapiFile.click();
});
document.querySelector("#btn-import-proto")!.addEventListener("click", () => {
  els.importProtoFile.click();
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

els.importOpenapiFile.addEventListener("change", async () => {
  const file = els.importOpenapiFile.files?.[0];
  if (!file) return;
  const json = await file.text();
  try {
    const created = await invoke<CollectionSummary>("import_openapi", { json });
    await refreshCollections();
    await openCollection(created.path);
  } catch (e) {
    alert(String(e));
  } finally {
    els.importOpenapiFile.value = "";
  }
});

els.importProtoFile.addEventListener("change", async () => {
  const file = els.importProtoFile.files?.[0];
  if (!file) return;
  const contents = await file.text();
  try {
    const created = await invoke<CollectionSummary>("import_proto", {
      contents,
      endpoint: "http://127.0.0.1:50051",
    });
    await refreshCollections();
    await openCollection(created.path);
  } catch (e) {
    alert(String(e));
  } finally {
    els.importProtoFile.value = "";
  }
});

const syncUrl = document.querySelector("#sync-url") as HTMLInputElement;
const syncEmail = document.querySelector("#sync-email") as HTMLInputElement;
const syncPassword = document.querySelector("#sync-password") as HTMLInputElement;
const syncStatus = document.querySelector("#sync-status") as HTMLSpanElement;

async function refreshSyncStatus() {
  try {
    const status = await invoke<{
      signedIn: boolean;
      email?: string | null;
      syncUrl?: string | null;
    }>("sync_status");
    syncStatus.textContent = status.signedIn
      ? `Signed in${status.email ? ` (${status.email})` : ""}`
      : "Local only";
    if (status.syncUrl) syncUrl.value = status.syncUrl;
  } catch {
    syncStatus.textContent = "Local only";
  }
}

document.querySelector("#btn-sync-login")!.addEventListener("click", async () => {
  try {
    await invoke("sync_login", {
      syncUrl: syncUrl.value,
      email: syncEmail.value,
      password: syncPassword.value,
    });
    await refreshSyncStatus();
  } catch (e) {
    alert(String(e));
  }
});

document.querySelector("#btn-sync-register")!.addEventListener("click", async () => {
  try {
    await invoke("sync_register", {
      syncUrl: syncUrl.value,
      email: syncEmail.value,
      password: syncPassword.value,
    });
    await refreshSyncStatus();
  } catch (e) {
    alert(String(e));
  }
});

document.querySelector("#btn-sync-now")!.addEventListener("click", async () => {
  try {
    const msg = await invoke<string>("sync_now", { force: true });
    await refreshCollections();
    els.responseBody.textContent = msg;
  } catch (e) {
    alert(String(e));
  }
});

async function boot() {
  await ensureDefaultEnvironment();
  await refreshEnvironments();
  await refreshCollections();
  await refreshSyncStatus();
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
