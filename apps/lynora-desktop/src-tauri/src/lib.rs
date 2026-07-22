use lynora_core::{
    import_postman_json, prepare_request, send_rest, Collection, Environment, Header,
    HistoryEntry, NewHistoryEntry, RequestDocument, Workspace,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

struct AppState {
    workspace: Workspace,
}

fn state_err(e: impl ToString) -> String {
    e.to_string()
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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendRequestInput {
    method: String,
    url: String,
    headers: Vec<Header>,
    body: Option<String>,
    environment_name: Option<String>,
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

#[tauri::command]
async fn send_request(
    state: tauri::State<'_, Mutex<AppState>>,
    input: SendRequestInput,
) -> Result<lynora_core::RestResponse, String> {
    let vars = {
        let state = state.lock().map_err(|e| e.to_string())?;
        let mut vars = HashMap::new();
        if let Some(name) = &input.environment_name {
            let envs = state.workspace.list_environments().map_err(state_err)?;
            if let Some(env) = envs.into_iter().find(|e| e.name == *name) {
                vars = env.values;
            }
        }
        vars
    };

    let doc = RequestDocument {
        id: String::new(),
        name: String::new(),
        method: input.method.clone(),
        url: input.url.clone(),
        headers: input.headers,
        body: input.body.clone(),
    };
    let prepared = prepare_request(&doc, &vars).map_err(state_err)?;
    let response = send_rest(prepared).await.map_err(state_err)?;

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
fn import_postman(
    state: tauri::State<'_, Mutex<AppState>>,
    json: String,
) -> Result<CollectionSummary, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    let slug_base = "imported";
    let mut root = state.workspace.collections_dir.join(slug_base);
    let mut n = 2;
    while root.exists() {
        root = state
            .workspace
            .collections_dir
            .join(format!("{slug_base}-{n}"));
        n += 1;
    }
    let col = import_postman_json(&json, &root).map_err(state_err)?;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let workspace = Workspace::open_default().expect("failed to open Lynora workspace");
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(AppState { workspace }))
        .invoke_handler(tauri::generate_handler![
            list_collections,
            create_collection,
            load_collection,
            save_request,
            list_environments,
            save_environment,
            send_request,
            import_postman,
            list_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
