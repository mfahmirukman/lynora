use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use lynora_sync::{AuthResponse, CollectionBundle, CollectionSummary};
use rusqlite::{params, Connection};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
}

fn hash_password(password: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}

fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            email TEXT UNIQUE NOT NULL,
            salt TEXT NOT NULL,
            password_hash TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS tokens (
            token TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            created_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS collections (
            user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            name TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            payload TEXT NOT NULL,
            PRIMARY KEY (user_id, id)
         );",
    )?;
    Ok(())
}

fn user_from_headers(state: &AppState, headers: &HeaderMap) -> Result<String, StatusCode> {
    let auth = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let token = auth
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let user_id: String = db
        .query_row(
            "SELECT user_id FROM tokens WHERE token = ?1",
            [token],
            |row| row.get(0),
        )
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok(user_id)
}

#[derive(Deserialize)]
struct AuthBody {
    email: String,
    password: String,
}

async fn register(
    State(state): State<AppState>,
    Json(body): Json<AuthBody>,
) -> Result<(StatusCode, Json<AuthResponse>), (StatusCode, String)> {
    let user_id = Uuid::new_v4().to_string();
    let salt = Uuid::new_v4().to_string();
    let password_hash = hash_password(&body.password, &salt);
    let token = Uuid::new_v4().to_string();
    {
        let db = state.db.lock().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        db.execute(
            "INSERT INTO users (id, email, salt, password_hash) VALUES (?1, ?2, ?3, ?4)",
            params![user_id, body.email, salt, password_hash],
        )
        .map_err(|e| (StatusCode::CONFLICT, e.to_string()))?;
        db.execute(
            "INSERT INTO tokens (token, user_id, created_at) VALUES (?1, ?2, ?3)",
            params![token, user_id, Utc::now().to_rfc3339()],
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            token,
            user_id,
            email: body.email,
        }),
    ))
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<AuthBody>,
) -> Result<Json<AuthResponse>, (StatusCode, String)> {
    let db = state.db.lock().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (user_id, salt, password_hash): (String, String, String) = db
        .query_row(
            "SELECT id, salt, password_hash FROM users WHERE email = ?1",
            [&body.email],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid credentials".into()))?;
    if hash_password(&body.password, &salt) != password_hash {
        return Err((StatusCode::UNAUTHORIZED, "invalid credentials".into()));
    }
    let token = Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO tokens (token, user_id, created_at) VALUES (?1, ?2, ?3)",
        params![token, user_id, Utc::now().to_rfc3339()],
    )
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(AuthResponse {
        token,
        user_id,
        email: body.email,
    }))
}

async fn list_collections(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<CollectionSummary>>, StatusCode> {
    let user_id = user_from_headers(&state, &headers)?;
    let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut stmt = db
        .prepare(
            "SELECT id, name, updated_at, content_hash FROM collections WHERE user_id = ?1 ORDER BY name",
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt
        .query_map([&user_id], |row| {
            let updated_at: String = row.get(2)?;
            Ok(CollectionSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                updated_at: updated_at
                    .parse()
                    .unwrap_or_else(|_| Utc::now()),
                content_hash: row.get(3)?,
            })
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?);
    }
    Ok(Json(out))
}

async fn get_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<CollectionBundle>, StatusCode> {
    let user_id = user_from_headers(&state, &headers)?;
    let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let payload: String = db
        .query_row(
            "SELECT payload FROM collections WHERE user_id = ?1 AND id = ?2",
            params![user_id, id],
            |row| row.get(0),
        )
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let bundle: CollectionBundle =
        serde_json::from_str(&payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(bundle))
}

#[derive(Deserialize)]
struct PushQuery {
    #[serde(default)]
    force: bool,
}

async fn put_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PushQuery>,
    Json(mut bundle): Json<CollectionBundle>,
) -> Result<Json<CollectionBundle>, (StatusCode, String)> {
    let user_id = user_from_headers(&state, &headers).map_err(|s| (s, "unauthorized".into()))?;
    if bundle.meta.id != id {
        bundle.meta.id = id.clone();
    }
    bundle.updated_at = Utc::now();
    let db = state.db.lock().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let existing: Option<(String, String)> = db
        .query_row(
            "SELECT content_hash, updated_at FROM collections WHERE user_id = ?1 AND id = ?2",
            params![user_id, id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    if let Some((hash, _updated)) = existing {
        if hash != bundle.content_hash && !query.force {
            return Err((
                StatusCode::CONFLICT,
                format!("remote hash {hash} differs; pass force=true to overwrite"),
            ));
        }
    }

    let payload = serde_json::to_string(&bundle).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    db.execute(
        "INSERT INTO collections (user_id, id, name, updated_at, content_hash, payload)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(user_id, id) DO UPDATE SET
           name=excluded.name,
           updated_at=excluded.updated_at,
           content_hash=excluded.content_hash,
           payload=excluded.payload",
        params![
            user_id,
            id,
            bundle.meta.name,
            bundle.updated_at.to_rfc3339(),
            bundle.content_hash,
            payload
        ],
    )
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(bundle))
}

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() {
    let db_path = std::env::var("LYNORA_SYNC_DB").unwrap_or_else(|_| "lynora-sync.db".into());
    let conn = Connection::open(&db_path).expect("open db");
    init_db(&conn).expect("init db");
    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/sync/collections", get(list_collections))
        .route(
            "/sync/collections/:id",
            get(get_collection).put(put_collection),
        )
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8787);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("lynora-sync-server listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}
