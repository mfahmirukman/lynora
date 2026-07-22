use crate::{LynoraError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectionMeta {
    pub id: String,
    pub name: String,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Header {
    pub key: String,
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestDocument {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub protocol: Protocol,
    #[serde(default)]
    pub auth: Option<crate::auth::AuthConfig>,
    #[serde(default)]
    pub graphql: Option<crate::graphql::GraphQlBody>,
    #[serde(default)]
    pub grpc: Option<crate::grpc::GrpcBody>,
    #[serde(default)]
    pub expect_status: Option<u16>,
    #[serde(default)]
    pub websocket: Option<crate::realtime::WebSocketBody>,
    #[serde(default)]
    pub sse: Option<crate::realtime::SseBody>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    #[default]
    Rest,
    Graphql,
    Grpc,
    Websocket,
    Sse,
}

#[derive(Debug, Clone)]
pub struct Collection {
    pub root: PathBuf,
    pub meta: CollectionMeta,
    pub requests: Vec<RequestDocument>,
}

impl Collection {
    pub fn create(root: &Path, name: &str) -> Result<Self> {
        fs::create_dir_all(root.join("requests"))?;
        let meta = CollectionMeta {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            version: 1,
        };
        let col = Self {
            root: root.to_path_buf(),
            meta: meta.clone(),
            requests: Vec::new(),
        };
        col.write_meta()?;
        Ok(col)
    }

    pub fn load(root: &Path) -> Result<Self> {
        let meta_path = root.join("lynora.json");
        if !meta_path.exists() {
            return Err(LynoraError::InvalidCollection(format!(
                "missing lynora.json in {}",
                root.display()
            )));
        }
        let meta: CollectionMeta = serde_json::from_str(&fs::read_to_string(meta_path)?)?;
        let mut requests = Vec::new();
        let req_dir = root.join("requests");
        if req_dir.exists() {
            for entry in fs::read_dir(&req_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    let doc: RequestDocument = serde_json::from_str(&fs::read_to_string(path)?)?;
                    requests.push(doc);
                }
            }
        }
        requests.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self {
            root: root.to_path_buf(),
            meta,
            requests,
        })
    }

    pub fn save_request(&mut self, req: &RequestDocument) -> Result<()> {
        let path = self.request_path(&req.id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serde_json::to_string_pretty(req)?)?;
        if let Some(existing) = self.requests.iter_mut().find(|r| r.id == req.id) {
            *existing = req.clone();
        } else {
            self.requests.push(req.clone());
            self.requests.sort_by(|a, b| a.name.cmp(&b.name));
        }
        Ok(())
    }

    pub fn delete_request(&mut self, id: &str) -> Result<()> {
        let path = self.request_path(id);
        if path.exists() {
            fs::remove_file(path)?;
        }
        self.requests.retain(|r| r.id != id);
        Ok(())
    }

    fn write_meta(&self) -> Result<()> {
        let path = self.root.join("lynora.json");
        fs::write(path, serde_json::to_string_pretty(&self.meta)?)?;
        Ok(())
    }

    fn request_path(&self, id: &str) -> PathBuf {
        self.root.join("requests").join(format!("{id}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_save_load_round_trip() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("my-api");
        let mut col = Collection::create(&root, "My API").unwrap();
        let req = RequestDocument {
            id: Uuid::new_v4().to_string(),
            name: "List users".into(),
            method: "GET".into(),
            url: "{{baseUrl}}/users".into(),
            headers: vec![Header {
                key: "Accept".into(),
                value: "application/json".into(),
                enabled: true,
            }],
            body: None,
            protocol: Protocol::Rest,
            auth: None,
            graphql: None,
            grpc: None,
            expect_status: None,
            websocket: None,
            sse: None,
        };
        col.save_request(&req).unwrap();
        let loaded = Collection::load(&root).unwrap();
        assert_eq!(loaded.meta.name, "My API");
        assert_eq!(loaded.requests.len(), 1);
        assert_eq!(loaded.requests[0], req);
    }
}
