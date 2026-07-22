use crate::collection::{Collection, Header, Protocol, RequestDocument};
use crate::{LynoraError, Result};
use serde_json::Value;
use std::path::Path;
use uuid::Uuid;

/// Import an OpenAPI 3.x JSON document into a Lynora collection (REST requests).
pub fn import_openapi_json(json: &str, dest_root: &Path) -> Result<Collection> {
    let root: Value = serde_json::from_str(json)
        .map_err(|e| LynoraError::Import(format!("invalid OpenAPI JSON: {e}")))?;

    let title = root
        .pointer("/info/title")
        .and_then(|v| v.as_str())
        .unwrap_or("OpenAPI Import");

    let base = root
        .pointer("/servers/0/url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_end_matches('/')
        .to_string();

    let mut col = Collection::create(dest_root, title)?;
    let paths = root
        .get("paths")
        .and_then(|p| p.as_object())
        .ok_or_else(|| LynoraError::Import("OpenAPI document missing paths".into()))?;

    for (path, item) in paths {
        let Some(item_obj) = item.as_object() else {
            continue;
        };
        for method in ["get", "post", "put", "patch", "delete", "head", "options"] {
            let Some(op) = item_obj.get(method) else {
                continue;
            };
            let name = op
                .get("operationId")
                .and_then(|v| v.as_str())
                .or_else(|| op.get("summary").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{} {}", method.to_uppercase(), path));

            let url = if base.is_empty() {
                path.clone()
            } else if path.starts_with('/') {
                format!("{base}{path}")
            } else {
                format!("{base}/{path}")
            };

            let mut headers = vec![Header {
                key: "Accept".into(),
                value: "application/json".into(),
                enabled: true,
            }];
            let body = if matches!(method, "post" | "put" | "patch") {
                headers.push(Header {
                    key: "Content-Type".into(),
                    value: "application/json".into(),
                    enabled: true,
                });
                Some("{}".into())
            } else {
                None
            };

            let doc = RequestDocument {
                id: Uuid::new_v4().to_string(),
                name,
                method: method.to_uppercase(),
                url,
                headers,
                body,
                protocol: Protocol::Rest,
                auth: None,
                graphql: None,
                grpc: None,
                expect_status: None,
                websocket: None,
                sse: None,
            };
            col.save_request(&doc)?;
        }
    }

    Ok(col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn imports_paths() {
        let json = r#"{
          "openapi": "3.0.3",
          "info": { "title": "Demo API", "version": "1.0.0" },
          "servers": [{ "url": "https://api.example.com" }],
          "paths": {
            "/users": {
              "get": { "operationId": "listUsers", "summary": "List users" },
              "post": { "operationId": "createUser" }
            }
          }
        }"#;
        let dir = tempdir().unwrap();
        let col = import_openapi_json(json, &dir.path().join("demo")).unwrap();
        assert_eq!(col.meta.name, "Demo API");
        assert_eq!(col.requests.len(), 2);
        assert!(col
            .requests
            .iter()
            .any(|r| r.method == "GET" && r.url.ends_with("/users")));
        assert!(col.requests.iter().any(|r| r.method == "POST"));
    }
}
