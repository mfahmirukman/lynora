import { invoke } from "@tauri-apps/api/core";

type Header = { key: string; value: string; enabled: boolean };

type CollectionSummary = { path: string; id: string; name: string };

type RequestDocument = {
  id: string;
  name: string;
  method: string;
  url: string;
  headers: Header[];
  body?: string | null;
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
  method: document.querySelector("#method") as HTMLSelectElement,
  url: document.querySelector("#url") as HTMLInputElement,
  name: document.querySelector("#req-name") as HTMLInputElement,
  headers: document.querySelector("#headers") as HTMLTextAreaElement,
  body: document.querySelector("#body") as HTMLTextAreaElement,
  status: document.querySelector("#status") as HTMLSpanElement,
  duration: document.querySelector("#duration") as HTMLSpanElement,
  responseBody: document.querySelector("#response-body") as HTMLPreElement,
  importFile: document.querySelector("#import-file") as HTMLInputElement,
};

let collections: CollectionSummary[] = [];
let activeCollection: CollectionDto | null = null;
let activeRequestId: string | null = null;

async function refreshCollections() {
  collections = await invoke<CollectionSummary[]>("list_collections");
  els.collectionList.innerHTML = "";
  for (const col of collections) {
    const li = document.createElement("li");
    const btn = document.createElement("button");
    btn.textContent = col.name;
    btn.className =
      activeCollection?.path === col.path ? "active" : "";
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
    btn.textContent = `${req.method} ${req.name}`;
    btn.className = activeRequestId === req.id ? "active" : "";
    btn.onclick = () => loadRequestIntoEditor(req);
    li.appendChild(btn);
    els.requestList.appendChild(li);
  }
}

function clearEditor() {
  activeRequestId = null;
  els.name.value = "Untitled";
  els.method.value = "GET";
  els.url.value = "";
  els.headers.value = JSON.stringify(
    [{ key: "Accept", value: "application/json", enabled: true }],
    null,
    2,
  );
  els.body.value = "";
  els.status.textContent = "—";
  els.duration.textContent = "";
  els.responseBody.textContent = "Send a request to see the response.";
}

function loadRequestIntoEditor(req: RequestDocument) {
  activeRequestId = req.id;
  els.name.value = req.name;
  els.method.value = req.method;
  els.url.value = req.url;
  els.headers.value = JSON.stringify(req.headers, null, 2);
  els.body.value = req.body ?? "";
  renderRequests();
}

function parseHeaders(): Header[] {
  try {
    const parsed = JSON.parse(els.headers.value || "[]");
    if (!Array.isArray(parsed)) throw new Error("headers must be an array");
    return parsed;
  } catch (e) {
    throw new Error(`Invalid headers JSON: ${e}`);
  }
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
    const saved = await invoke<RequestDocument>("save_request", {
      input: {
        collectionPath: activeCollection.path,
        id: activeRequestId,
        name: els.name.value || "Untitled",
        method: els.method.value,
        url: els.url.value,
        headers: parseHeaders(),
        body: els.body.value || null,
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
      input: {
        method: els.method.value,
        url: els.url.value,
        headers: parseHeaders(),
        body: els.body.value || null,
        environmentName: els.envSelect.value || null,
      },
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
  if (collections[0]) {
    await openCollection(collections[0].path);
  } else {
    clearEditor();
  }
}

boot().catch((e) => {
  els.responseBody.textContent = `Failed to start: ${e}`;
});
