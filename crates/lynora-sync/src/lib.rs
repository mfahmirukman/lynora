//! Optional collection sync for Lynora (no plaintext secrets).

use chrono::{DateTime, Utc};
use lynora_core::{Collection, CollectionMeta, Environment, RequestDocument};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, SyncError>;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("core error: {0}")]
    Core(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("auth error: {0}")]
    Auth(String),
    #[error("{0}")]
    Message(String),
}

impl From<lynora_core::LynoraError> for SyncError {
    fn from(value: lynora_core::LynoraError) -> Self {
        SyncError::Core(value.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub token: String,
    pub user_id: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionBundle {
    pub meta: CollectionMeta,
    pub requests: Vec<RequestDocument>,
    /// Non-secret environment values only.
    #[serde(default)]
    pub environments: Vec<Environment>,
    pub updated_at: DateTime<Utc>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    pub id: String,
    pub name: String,
    pub updated_at: DateTime<Utc>,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
pub struct SyncClient {
    pub base_url: String,
    pub token: Option<String>,
    http: reqwest::Client,
}

impl SyncClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: None,
            http: reqwest::Client::new(),
        }
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn set_token(&mut self, token: Option<String>) {
        self.token = token;
    }

    async fn authed(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(token) = &self.token {
            req.header("Authorization", format!("Bearer {token}"))
        } else {
            req
        }
    }

    pub async fn register(&mut self, email: &str, password: &str) -> Result<AuthResponse> {
        let url = format!("{}/auth/register", self.base_url);
        let resp = self
            .http
            .post(url)
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(SyncError::Auth(resp.text().await.unwrap_or_default()));
        }
        let auth: AuthResponse = resp.json().await?;
        self.token = Some(auth.token.clone());
        Ok(auth)
    }

    pub async fn login(&mut self, email: &str, password: &str) -> Result<AuthResponse> {
        let url = format!("{}/auth/login", self.base_url);
        let resp = self
            .http
            .post(url)
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(SyncError::Auth(resp.text().await.unwrap_or_default()));
        }
        let auth: AuthResponse = resp.json().await?;
        self.token = Some(auth.token.clone());
        Ok(auth)
    }

    pub async fn list_remote(&self) -> Result<Vec<CollectionSummary>> {
        let url = format!("{}/sync/collections", self.base_url);
        let req = self.authed(self.http.get(url)).await;
        let resp = req.send().await?;
        if resp.status().as_u16() == 401 {
            return Err(SyncError::Auth("not signed in".into()));
        }
        if !resp.status().is_success() {
            return Err(SyncError::Message(resp.text().await.unwrap_or_default()));
        }
        Ok(resp.json().await?)
    }

    pub async fn pull(&self, collection_id: &str) -> Result<CollectionBundle> {
        let url = format!("{}/sync/collections/{collection_id}", self.base_url);
        let req = self.authed(self.http.get(url)).await;
        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Err(SyncError::Message(resp.text().await.unwrap_or_default()));
        }
        Ok(resp.json().await?)
    }

    pub async fn push(&self, bundle: &CollectionBundle, force: bool) -> Result<CollectionBundle> {
        let url = format!(
            "{}/sync/collections/{}?force={}",
            self.base_url, bundle.meta.id, force
        );
        let req = self.authed(self.http.put(url).json(bundle)).await;
        let resp = req.send().await?;
        if resp.status().as_u16() == 409 {
            return Err(SyncError::Conflict(resp.text().await.unwrap_or_default()));
        }
        if !resp.status().is_success() {
            return Err(SyncError::Message(resp.text().await.unwrap_or_default()));
        }
        Ok(resp.json().await?)
    }
}

/// Strip secret keys from environments before sync.
pub fn redact_environments(envs: &[Environment]) -> Vec<Environment> {
    envs.iter()
        .map(|env| {
            let mut values = HashMap::new();
            for (k, v) in &env.values {
                if env.secrets.iter().any(|s| s == k) {
                    continue;
                }
                values.insert(k.clone(), v.clone());
            }
            Environment {
                name: env.name.clone(),
                values,
                secrets: env.secrets.clone(),
            }
        })
        .collect()
}

pub fn hash_bundle_content(meta: &CollectionMeta, requests: &[RequestDocument]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_string(meta).unwrap_or_default().as_bytes());
    let mut reqs = requests.to_vec();
    reqs.sort_by(|a, b| a.id.cmp(&b.id));
    hasher.update(serde_json::to_string(&reqs).unwrap_or_default().as_bytes());
    hex::encode(hasher.finalize())
}

pub fn bundle_from_collection(
    col: &Collection,
    envs: &[Environment],
) -> Result<CollectionBundle> {
    let updated_at = Utc::now();
    let content_hash = hash_bundle_content(&col.meta, &col.requests);
    Ok(CollectionBundle {
        meta: col.meta.clone(),
        requests: col.requests.clone(),
        environments: redact_environments(envs),
        updated_at,
        content_hash,
    })
}

/// Write bundle onto disk; if target exists and hashes differ, copy to `.bak-<ts>` first.
pub fn apply_bundle_to_disk(bundle: &CollectionBundle, root: &Path) -> Result<Option<PathBuf>> {
    let mut backup = None;
    if root.join("lynora.json").exists() {
        let existing = Collection::load(root)?;
        let existing_hash = hash_bundle_content(&existing.meta, &existing.requests);
        if existing_hash != bundle.content_hash {
            let ts = Utc::now().format("%Y%m%d%H%M%S");
            let bak = root
                .parent()
                .unwrap_or(root)
                .join(format!("{}-bak-{ts}", root.file_name().unwrap().to_string_lossy()));
            copy_dir(root, &bak)?;
            backup = Some(bak);
        }
    }

    if root.exists() {
        // Replace requests carefully
        let _ = fs::remove_dir_all(root.join("requests"));
    }
    fs::create_dir_all(root.join("requests"))?;
    fs::write(
        root.join("lynora.json"),
        serde_json::to_string_pretty(&bundle.meta)?,
    )?;
    for req in &bundle.requests {
        // Never persist Authorization-looking secrets from sync beyond request files as stored
        let mut req = req.clone();
        scrub_request_secrets(&mut req);
        fs::write(
            root.join("requests").join(format!("{}.json", req.id)),
            serde_json::to_string_pretty(&req)?,
        )?;
    }
    Ok(backup)
}

fn scrub_request_secrets(req: &mut RequestDocument) {
    if let Some(auth) = req.auth.as_mut() {
        auth.token = None;
        auth.password = None;
        auth.secret_access_key = None;
        auth.session_token = None;
    }
    req.headers.retain(|h| {
        !h.key.eq_ignore_ascii_case("authorization")
            && !h.key.to_ascii_lowercase().contains("api-key")
    });
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lynora_core::{Protocol, RequestDocument};
    use uuid::Uuid;

    #[test]
    fn redacts_secret_env_values() {
        let mut values = HashMap::new();
        values.insert("baseUrl".into(), "http://x".into());
        values.insert("token".into(), "sekrit".into());
        let env = Environment {
            name: "local".into(),
            values,
            secrets: vec!["token".into()],
        };
        let redacted = redact_environments(&[env]);
        assert!(redacted[0].values.contains_key("baseUrl"));
        assert!(!redacted[0].values.contains_key("token"));
    }

    #[test]
    fn hash_stable_for_same_content() {
        let meta = CollectionMeta {
            id: "1".into(),
            name: "A".into(),
            version: 1,
        };
        let req = RequestDocument {
            id: Uuid::new_v4().to_string(),
            name: "r".into(),
            method: "GET".into(),
            url: "http://x".into(),
            headers: vec![],
            body: None,
            protocol: Protocol::Rest,
            auth: None,
            graphql: None,
            grpc: None,
        };
        let h1 = hash_bundle_content(&meta, &[req.clone()]);
        let h2 = hash_bundle_content(&meta, &[req]);
        assert_eq!(h1, h2);
    }
}
