use crate::rest::{send as send_rest, RestRequest, RestResponse};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlBody {
    pub query: String,
    #[serde(default)]
    pub variables: Option<String>,
    #[serde(default)]
    pub operation_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GraphQlRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: GraphQlBody,
}

pub fn build_payload(body: &GraphQlBody) -> Result<String> {
    let variables = match &body.variables {
        Some(v) if !v.trim().is_empty() => serde_json::from_str(v)?,
        _ => json!({}),
    };
    let mut payload = json!({
        "query": body.query,
        "variables": variables,
    });
    if let Some(op) = &body.operation_name {
        if !op.is_empty() {
            payload["operationName"] = json!(op);
        }
    }
    Ok(serde_json::to_string(&payload)?)
}

pub async fn send(req: GraphQlRequest) -> Result<RestResponse> {
    let mut headers = req.headers;
    if !headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
    {
        headers.push(("Content-Type".into(), "application/json".into()));
    }
    let body = build_payload(&req.body)?;
    send_rest(RestRequest {
        method: "POST".into(),
        url: req.url,
        headers,
        body: Some(body),
    })
    .await
}

const INTROSPECTION_QUERY: &str = r#"
query IntrospectionQuery {
  __schema {
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types {
      kind
      name
      description
      fields(includeDeprecated: true) {
        name
        description
        args {
          name
          description
          type { kind name ofType { kind name ofType { kind name ofType { kind name } } } }
          defaultValue
        }
        type { kind name ofType { kind name ofType { kind name ofType { kind name } } } }
        isDeprecated
        deprecationReason
      }
    }
  }
}
"#;

pub async fn introspect(
    url: &str,
    headers: Vec<(String, String)>,
) -> Result<RestResponse> {
    send(GraphQlRequest {
        url: url.to_string(),
        headers,
        body: GraphQlBody {
            query: INTROSPECTION_QUERY.trim().to_string(),
            variables: None,
            operation_name: Some("IntrospectionQuery".into()),
        },
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn payload_includes_variables() {
        let body = GraphQlBody {
            query: "query Q($id: ID!){ user(id:$id){ name } }".into(),
            variables: Some(r#"{"id":"1"}"#.into()),
            operation_name: Some("Q".into()),
        };
        let payload = build_payload(&body).unwrap();
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(v["operationName"], "Q");
        assert_eq!(v["variables"]["id"], "1");
    }

    #[tokio::test]
    async fn send_posts_json() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);
            assert!(req.contains("POST"));
            assert!(req.contains("query"));
            let body = br#"{"data":{"ok":true}}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(resp.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });

        let resp = send(GraphQlRequest {
            url: format!("http://{addr}/graphql"),
            headers: vec![],
            body: GraphQlBody {
                query: "{ ok }".into(),
                variables: None,
                operation_name: None,
            },
        })
        .await
        .unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("ok"));
    }
}
