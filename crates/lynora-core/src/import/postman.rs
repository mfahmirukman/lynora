use crate::collection::{Collection, Header, RequestDocument};
use crate::{LynoraError, Result};
use serde::Deserialize;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
struct PostmanCollection {
    info: PostmanInfo,
    #[serde(default)]
    item: Vec<PostmanItem>,
}

#[derive(Debug, Clone, Deserialize)]
struct PostmanInfo {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum PostmanItem {
    Folder {
        name: String,
        item: Vec<PostmanItem>,
    },
    Request {
        name: String,
        request: PostmanRequest,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct PostmanRequest {
    method: String,
    url: PostmanUrl,
    #[serde(default)]
    header: Vec<PostmanHeader>,
    #[serde(default)]
    body: Option<PostmanBody>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum PostmanUrl {
    Raw(String),
    Object {
        raw: Option<String>,
        #[serde(default)]
        host: Vec<String>,
        #[serde(default)]
        path: Vec<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct PostmanHeader {
    key: String,
    #[serde(default)]
    value: String,
    #[serde(default)]
    disabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct PostmanBody {
    #[serde(default)]
    mode: String,
    #[serde(default)]
    raw: Option<String>,
}

/// Import a Postman Collection v2.1 JSON document into a new Lynora collection directory.
pub fn import_postman_json(json: &str, dest_root: &Path) -> Result<Collection> {
    let parsed: PostmanCollection = serde_json::from_str(json)
        .map_err(|e| LynoraError::Import(format!("invalid Postman JSON: {e}")))?;

    let mut col = Collection::create(dest_root, &parsed.info.name)?;
    let mut flat = Vec::new();
    flatten_items(&parsed.item, "", &mut flat);

    for (name, req) in flat {
        let url = resolve_url(&req.url)?;
        let headers = req
            .header
            .iter()
            .map(|h| Header {
                key: h.key.clone(),
                value: h.value.clone(),
                enabled: !h.disabled,
            })
            .collect();
        let body = req.body.as_ref().and_then(|b| {
            if b.mode == "raw" || b.raw.is_some() {
                b.raw.clone()
            } else {
                None
            }
        });
        let doc = RequestDocument {
            id: Uuid::new_v4().to_string(),
            name,
            method: req.method,
            url,
            headers,
            body,
            protocol: crate::collection::Protocol::Rest,
            auth: None,
            graphql: None,
            grpc: None,
        };
        col.save_request(&doc)?;
    }

    Ok(col)
}

fn flatten_items(items: &[PostmanItem], prefix: &str, out: &mut Vec<(String, PostmanRequest)>) {
    for item in items {
        match item {
            PostmanItem::Folder { name, item } => {
                let next = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix} / {name}")
                };
                flatten_items(item, &next, out);
            }
            PostmanItem::Request { name, request } => {
                let full = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix} / {name}")
                };
                out.push((full, request.clone()));
            }
        }
    }
}

fn resolve_url(url: &PostmanUrl) -> Result<String> {
    match url {
        PostmanUrl::Raw(s) => Ok(s.clone()),
        PostmanUrl::Object { raw, host, path } => {
            if let Some(r) = raw {
                if !r.is_empty() {
                    return Ok(r.clone());
                }
            }
            let host = host.join(".");
            let path = path.join("/");
            if host.is_empty() {
                return Err(LynoraError::Import("request URL missing".into()));
            }
            if path.is_empty() {
                Ok(host)
            } else {
                Ok(format!("{host}/{path}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn imports_simple_fixture() {
        let json = include_str!("../../tests/fixtures/postman_simple.json");
        let dir = tempdir().unwrap();
        let dest = dir.path().join("imported");
        let col = import_postman_json(json, &dest).unwrap();
        assert_eq!(col.meta.name, "Lynora Sample");
        assert_eq!(col.requests.len(), 2);
        let get_users = col
            .requests
            .iter()
            .find(|r| r.name == "Users / List users")
            .unwrap();
        assert_eq!(get_users.method, "GET");
        assert!(get_users.url.contains("/users"));
        let create = col
            .requests
            .iter()
            .find(|r| r.name.contains("Create"))
            .unwrap();
        assert_eq!(create.method, "POST");
        assert!(create.body.as_ref().unwrap().contains("name"));
    }
}
