use lynora_core::{
    generate_code, import_openapi_json, import_postman_json, import_proto_source, introspect_graphql,
    prepare_request, send_graphql, send_grpc, send_rest, AuthConfig, CodeLanguage, Collection,
    Environment, GraphQlBody, GraphQlRequest, GrpcBody, GrpcRequest, Header, HistoryEntry,
    NewHistoryEntry, Protocol, RequestDocument, RestRequest, Workspace,
};
use lynora_sync::{
    apply_bundle_to_disk, bundle_from_collection, SyncClient,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

struct AppState {
    workspace: Workspace,
    sync: Option<SyncClient>,
}

fn state_err(e: impl ToString) -> String {
    e.to_string()
}

fn load_env_vars(
    state: &AppState,
    environment_name: &Option<String>,
) -> Result<HashMap<String, String>, String> {
    let mut vars = HashMap::new();
    if let Some(name) = environment_name {
        let envs = state.workspace.list_environments().map_err(state_err)?;
        if let Some(env) = envs.into_iter().find(|e| e.name == *name) {
            vars = env.values;
        }
    }
    Ok(vars)
}

#[derive(Debug, Serialize)]
struct CollectionSummary {
    path: String,
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveRequestInput {
    collection_path: String,
    id: Option<String>,
    name: String,
    method: String,
    url: String,
    headers: Vec<Header>,
    body: Option<String>,
    #[serde(default)]
    protocol: Protocol,
    #[serde(default)]
    auth: Option<AuthConfig>,
    #[serde(default)]
    graphql: Option<GraphQlBody>,
    #[serde(default)]
    grpc: Option<GrpcBody>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendRequestInput {
    method: String,
    url: String,
    headers: Vec<Header>,
    body: Option<String>,
    environment_name: Option<String>,
    #[serde(default)]
    protocol: Protocol,
    #[serde(default)]
    auth: Option<AuthConfig>,
    #[serde(default)]
    graphql: Option<GraphQlBody>,
    #[serde(default)]
    grpc: Option<GrpcBody>,
    #[serde(default)]
    collection_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateCodeInput {
    language: CodeLanguage,
    method: String,
    url: String,
    headers: Vec<Header>,
    body: Option<String>,
    environment_name: Option<String>,
    #[serde(default)]
    auth: Option<AuthConfig>,
    #[serde(default)]
    protocol: Protocol,
    #[serde(default)]
    graphql: Option<GraphQlBody>,
    #[serde(default)]
    grpc: Option<GrpcBody>,
}

#[tauri::command]
fn list_collections(state: tauri::State<'_, Mutex<AppState>>) -> Result<Vec<CollectionSummary>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let list = state.workspace.list_collections().map_err(state_err)?;
    Ok(list
        .into_iter()
        .map(|(path, meta)| CollectionSummary {
            path: path.display().to_string(),
            id: meta.id,
            name: meta.name,
        })
        .collect())
}

#[tauri::command]
fn create_collection(
    state: tauri::State<'_, Mutex<AppState>>,
    name: String,
) -> Result<CollectionSummary, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let col = state.workspace.create_collection(&name).map_err(state_err)?;
    Ok(CollectionSummary {
        path: col.root.display().to_string(),
        id: col.meta.id,
        name: col.meta.name,
    })
}

#[tauri::command]
fn load_collection(path: String) -> Result<CollectionDto, String> {
    let col = Collection::load(PathBuf::from(path).as_path()).map_err(state_err)?;
    Ok(CollectionDto::from(col))
}

#[derive(Debug, Serialize)]
struct CollectionDto {
    path: String,
    id: String,
    name: String,
    requests: Vec<RequestDocument>,
}

impl From<Collection> for CollectionDto {
    fn from(col: Collection) -> Self {
        Self {
            path: col.root.display().to_string(),
            id: col.meta.id,
            name: col.meta.name,
            requests: col.requests,
        }
    }
}

#[tauri::command]
fn save_request(input: SaveRequestInput) -> Result<RequestDocument, String> {
    let mut col = Collection::load(PathBuf::from(&input.collection_path).as_path())
        .map_err(state_err)?;
    let doc = RequestDocument {
        id: input.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: input.name,
        method: input.method,
        url: input.url,
        headers: input.headers,
        body: input.body,
        protocol: input.protocol,
        auth: input.auth,
        graphql: input.graphql,
        grpc: input.grpc,
    };
    col.save_request(&doc).map_err(state_err)?;
    Ok(doc)
}

#[tauri::command]
fn list_environments(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<Vec<Environment>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    state.workspace.list_environments().map_err(state_err)
}

#[tauri::command]
fn save_environment(
    state: tauri::State<'_, Mutex<AppState>>,
    env: Environment,
) -> Result<(), String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let path = state
        .workspace
        .environments_dir()
        .join(format!("{}.json", env.name));
    env.save(&path).map_err(state_err)
}

fn doc_from_send(input: &SendRequestInput) -> RequestDocument {
    RequestDocument {
        id: String::new(),
        name: String::new(),
        method: input.method.clone(),
        url: input.url.clone(),
        headers: input.headers.clone(),
        body: input.body.clone(),
        protocol: input.protocol.clone(),
        auth: input.auth.clone(),
        graphql: input.graphql.clone(),
        grpc: input.grpc.clone(),
    }
}

async fn dispatch_send(
    doc: &RequestDocument,
    vars: &HashMap<String, String>,
    collection_path: Option<&str>,
) -> Result<lynora_core::RestResponse, String> {
    match doc.protocol {
        Protocol::Graphql => {
            let prepared = prepare_request(doc, vars).map_err(state_err)?;
            let mut gql = doc.graphql.clone().unwrap_or(GraphQlBody {
                query: doc.body.clone().unwrap_or_default(),
                variables: None,
                operation_name: None,
            });
            if let Some(vars_json) = gql.variables.as_ref() {
                gql.variables = Some(lynora_core::expand(vars_json, vars).map_err(state_err)?);
            }
            gql.query = lynora_core::expand(&gql.query, vars).map_err(state_err)?;
            send_graphql(GraphQlRequest {
                url: prepared.url,
                headers: prepared.headers,
                body: gql,
            })
            .await
            .map_err(state_err)
        }
        Protocol::Grpc => {
            let prepared = prepare_request(doc, vars).map_err(state_err)?;
            let mut grpc_body = doc.grpc.clone().ok_or_else(|| {
                "gRPC request missing service/method metadata".to_string()
            })?;
            grpc_body.message_json =
                lynora_core::expand(&grpc_body.message_json, vars).map_err(state_err)?;
            send_grpc(GrpcRequest {
                endpoint: prepared.url,
                body: grpc_body,
                collection_root: collection_path.map(PathBuf::from),
                headers: prepared.headers,
            })
            .await
            .map_err(state_err)
        }
        Protocol::Rest => {
            let prepared = prepare_request(doc, vars).map_err(state_err)?;
            send_rest(prepared).await.map_err(state_err)
        }
    }
}

#[tauri::command]
async fn send_request(
    state: tauri::State<'_, Mutex<AppState>>,
    input: SendRequestInput,
) -> Result<lynora_core::RestResponse, String> {
    let vars = {
        let state = state.lock().map_err(|e| e.to_string())?;
        load_env_vars(&state, &input.environment_name)?
    };
    let doc = doc_from_send(&input);
    let response = dispatch_send(&doc, &vars, input.collection_path.as_deref()).await?;

    {
        let state = state.lock().map_err(|e| e.to_string())?;
        let history = state.workspace.history().map_err(state_err)?;
        let _ = history.append(NewHistoryEntry::from_exchange(
            &input.method,
            &input.url,
            input.body,
            &response,
        ));
    }

    Ok(response)
}

#[tauri::command]
async fn generate_snippet(
    state: tauri::State<'_, Mutex<AppState>>,
    input: GenerateCodeInput,
) -> Result<String, String> {
    let vars = {
        let state = state.lock().map_err(|e| e.to_string())?;
        load_env_vars(&state, &input.environment_name)?
    };
    let doc = RequestDocument {
        id: String::new(),
        name: String::new(),
        method: input.method,
        url: input.url,
        headers: input.headers,
        body: input.body,
        protocol: input.protocol,
        auth: input.auth,
        graphql: input.graphql.clone(),
        grpc: input.grpc.clone(),
    };

    let prepared = match doc.protocol {
        Protocol::Graphql => {
            let base = prepare_request(&doc, &vars).map_err(state_err)?;
            let gql = doc.graphql.unwrap_or(GraphQlBody {
                query: base.body.clone().unwrap_or_default(),
                variables: None,
                operation_name: None,
            });
            let payload = lynora_core::graphql::build_payload(&gql).map_err(state_err)?;
            RestRequest {
                method: "POST".into(),
                url: base.url,
                headers: {
                    let mut h = base.headers;
                    if !h.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
                        h.push(("Content-Type".into(), "application/json".into()));
                    }
                    h
                },
                body: Some(payload),
            }
        }
        Protocol::Grpc => {
            let base = prepare_request(&doc, &vars).map_err(state_err)?;
            let grpc = doc.grpc.unwrap_or_default();
            RestRequest {
                method: "POST".into(),
                url: format!("{}/{}", base.url.trim_end_matches('/'), grpc.method),
                headers: base.headers,
                body: Some(grpc.message_json),
            }
        }
        Protocol::Rest => prepare_request(&doc, &vars).map_err(state_err)?,
    };

    Ok(generate_code(input.language, &prepared))
}

#[tauri::command]
async fn introspect(
    state: tauri::State<'_, Mutex<AppState>>,
    url: String,
    headers: Vec<Header>,
    environment_name: Option<String>,
    auth: Option<AuthConfig>,
) -> Result<lynora_core::RestResponse, String> {
    let vars = {
        let state = state.lock().map_err(|e| e.to_string())?;
        load_env_vars(&state, &environment_name)?
    };
    let doc = RequestDocument {
        id: String::new(),
        name: String::new(),
        method: "POST".into(),
        url,
        headers,
        body: None,
        protocol: Protocol::Graphql,
        auth,
        graphql: None,
        grpc: None,
    };
    let prepared = prepare_request(&doc, &vars).map_err(state_err)?;
    introspect_graphql(&prepared.url, prepared.headers)
        .await
        .map_err(state_err)
}

fn next_import_root(state: &AppState, slug_base: &str) -> PathBuf {
    let mut root = state.workspace.collections_dir.join(slug_base);
    let mut n = 2;
    while root.exists() {
        root = state
            .workspace
            .collections_dir
            .join(format!("{slug_base}-{n}"));
        n += 1;
    }
    root
}

#[tauri::command]
fn import_postman(
    state: tauri::State<'_, Mutex<AppState>>,
    json: String,
) -> Result<CollectionSummary, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let root = next_import_root(&state, "imported");
    let col = import_postman_json(&json, &root).map_err(state_err)?;
    Ok(CollectionSummary {
        path: col.root.display().to_string(),
        id: col.meta.id,
        name: col.meta.name,
    })
}

#[tauri::command]
fn import_openapi(
    state: tauri::State<'_, Mutex<AppState>>,
    json: String,
) -> Result<CollectionSummary, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let root = next_import_root(&state, "openapi");
    let col = import_openapi_json(&json, &root).map_err(state_err)?;
    Ok(CollectionSummary {
        path: col.root.display().to_string(),
        id: col.meta.id,
        name: col.meta.name,
    })
}

#[tauri::command]
fn import_proto(
    state: tauri::State<'_, Mutex<AppState>>,
    contents: String,
    endpoint: Option<String>,
) -> Result<CollectionSummary, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let root = next_import_root(&state, "proto");
    let endpoint = endpoint.unwrap_or_else(|| "http://127.0.0.1:50051".into());
    let col = import_proto_source(&contents, &root, &endpoint).map_err(state_err)?;
    Ok(CollectionSummary {
        path: col.root.display().to_string(),
        id: col.meta.id,
        name: col.meta.name,
    })
}

#[tauri::command]
fn list_history(
    state: tauri::State<'_, Mutex<AppState>>,
    limit: Option<usize>,
) -> Result<Vec<HistoryEntry>, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let history = state.workspace.history().map_err(state_err)?;
    history.list_recent(limit.unwrap_or(50)).map_err(state_err)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthStatus {
    signed_in: bool,
    email: Option<String>,
    sync_url: Option<String>,
}

#[tauri::command]
async fn sync_register(
    state: tauri::State<'_, Mutex<AppState>>,
    sync_url: String,
    email: String,
    password: String,
) -> Result<AuthStatus, String> {
    let mut client = SyncClient::new(sync_url.clone());
    let auth = client
        .register(&email, &password)
        .await
        .map_err(|e| e.to_string())?;
    {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.sync = Some(client);
        let _ = std::fs::write(
            state.workspace.config_dir.join("sync.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "syncUrl": sync_url,
                "token": auth.token,
                "email": auth.email,
            }))
            .unwrap_or_default(),
        );
    }
    Ok(AuthStatus {
        signed_in: true,
        email: Some(auth.email),
        sync_url: Some(sync_url),
    })
}

#[tauri::command]
async fn sync_login(
    state: tauri::State<'_, Mutex<AppState>>,
    sync_url: String,
    email: String,
    password: String,
) -> Result<AuthStatus, String> {
    let mut client = SyncClient::new(sync_url.clone());
    let auth = client
        .login(&email, &password)
        .await
        .map_err(|e| e.to_string())?;
    {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.sync = Some(client);
        let _ = std::fs::write(
            state.workspace.config_dir.join("sync.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "syncUrl": sync_url,
                "token": auth.token,
                "email": auth.email,
            }))
            .unwrap_or_default(),
        );
    }
    Ok(AuthStatus {
        signed_in: true,
        email: Some(auth.email),
        sync_url: Some(sync_url),
    })
}

#[tauri::command]
fn sync_status(state: tauri::State<'_, Mutex<AppState>>) -> Result<AuthStatus, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let path = state.workspace.config_dir.join("sync.json");
    if !path.exists() {
        return Ok(AuthStatus {
            signed_in: false,
            email: None,
            sync_url: None,
        });
    }
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).map_err(state_err)?)
            .map_err(state_err)?;
    Ok(AuthStatus {
        signed_in: v.get("token").and_then(|t| t.as_str()).is_some(),
        email: v.get("email").and_then(|e| e.as_str()).map(str::to_string),
        sync_url: v
            .get("syncUrl")
            .and_then(|e| e.as_str())
            .map(str::to_string),
    })
}

#[tauri::command]
async fn sync_now(
    state: tauri::State<'_, Mutex<AppState>>,
    force: Option<bool>,
) -> Result<String, String> {
    let force = force.unwrap_or(false);
    let (client, collections, envs, collections_dir) = {
        let state = state.lock().map_err(|e| e.to_string())?;
        let path = state.workspace.config_dir.join("sync.json");
        let v: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path).map_err(|_| "not signed in — save sync.json via login".to_string())?,
        )
        .map_err(state_err)?;
        let url = v
            .get("syncUrl")
            .and_then(|u| u.as_str())
            .ok_or_else(|| "missing syncUrl".to_string())?;
        let token = v
            .get("token")
            .and_then(|t| t.as_str())
            .ok_or_else(|| "missing token".to_string())?;
        let client = SyncClient::new(url).with_token(token);
        let collections = state.workspace.list_collections().map_err(state_err)?;
        let envs = state.workspace.list_environments().map_err(state_err)?;
        (
            client,
            collections,
            envs,
            state.workspace.collections_dir.clone(),
        )
    };

    let mut pushed = 0usize;
    for (path, _meta) in &collections {
        let col = Collection::load(path).map_err(state_err)?;
        let bundle = bundle_from_collection(&col, &envs).map_err(|e| e.to_string())?;
        client
            .push(&bundle, force)
            .await
            .map_err(|e| e.to_string())?;
        pushed += 1;
    }

    let remote = client.list_remote().await.map_err(|e| e.to_string())?;
    let mut pulled = 0usize;
    for item in remote {
        let local = collections.iter().find(|(_, m)| m.id == item.id);
        if local.is_some() && !force {
            continue;
        }
        let bundle = client.pull(&item.id).await.map_err(|e| e.to_string())?;
        let dest = if let Some((path, _)) = local {
            path.clone()
        } else {
            collections_dir.join(item.name.replace(['/', ' '], "-").to_lowercase())
        };
        let _backup = apply_bundle_to_disk(&bundle, &dest).map_err(|e| e.to_string())?;
        pulled += 1;
    }

    Ok(format!("pushed {pushed}, pulled/merged {pulled}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let workspace = Workspace::open_default().expect("failed to open Lynora workspace");
    // Restore sync client if present
    let sync_client = {
        let path = workspace.config_dir.join("sync.json");
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| {
                    let url = v.get("syncUrl")?.as_str()?;
                    let token = v.get("token")?.as_str()?;
                    Some(SyncClient::new(url).with_token(token))
                })
        } else {
            None
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(AppState {
            workspace,
            sync: sync_client,
        }))
        .invoke_handler(tauri::generate_handler![
            list_collections,
            create_collection,
            load_collection,
            save_request,
            list_environments,
            save_environment,
            send_request,
            generate_snippet,
            introspect,
            import_postman,
            import_openapi,
            import_proto,
            list_history,
            sync_register,
            sync_login,
            sync_status,
            sync_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
