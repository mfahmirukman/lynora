use crate::auth::{self, AuthConfig, AuthKind};
use crate::collection::RequestDocument;
use crate::vars;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct RestRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub duration_ms: u128,
}

pub fn prepare_request(
    doc: &RequestDocument,
    vars_map: &HashMap<String, String>,
) -> Result<RestRequest> {
    let url = vars::expand(&doc.url, vars_map)?;
    let mut headers = Vec::new();
    for h in &doc.headers {
        if !h.enabled {
            continue;
        }
        headers.push((h.key.clone(), vars::expand(&h.value, vars_map)?));
    }
    let body = match &doc.body {
        Some(b) => Some(vars::expand(b, vars_map)?),
        None => None,
    };

    let mut req = RestRequest {
        method: doc.method.clone(),
        url,
        headers,
        body,
    };

    if let Some(auth) = &doc.auth {
        apply_auth_to_request(&mut req, auth, vars_map)?;
    }

    Ok(req)
}

pub fn apply_auth_to_request(
    req: &mut RestRequest,
    auth: &AuthConfig,
    vars_map: &HashMap<String, String>,
) -> Result<()> {
    let auth = auth::expand_auth(auth, vars_map)?;
    let (url, headers) = auth::apply_auth_headers(req.headers.clone(), req.url.clone(), &auth)?;
    req.url = url;
    req.headers = headers;
    if matches!(auth.kind, AuthKind::AwsSigV4) {
        auth::apply_aws_sigv4(
            &req.method,
            &req.url,
            &mut req.headers,
            req.body.as_deref(),
            &auth,
        )?;
    }
    Ok(())
}

pub async fn send(req: RestRequest) -> Result<RestResponse> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let method =
        reqwest::Method::from_bytes(req.method.as_bytes()).unwrap_or(reqwest::Method::GET);

    let mut builder = client.request(method, &req.url);
    for (k, v) in &req.headers {
        builder = builder.header(k, v);
    }
    if let Some(body) = &req.body {
        builder = builder.body(body.clone());
    }

    let started = Instant::now();
    let response = builder.send().await?;
    let duration_ms = started.elapsed().as_millis();
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                v.to_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    let body = response.text().await?;

    Ok(RestResponse {
        status,
        headers,
        body,
        duration_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthConfig, AuthKind};
    use crate::collection::{Header, Protocol};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn spawn_hello_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = b"hello-lynora";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(resp.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });
        format!("http://{addr}/")
    }

    #[tokio::test]
    async fn send_get_local_server() {
        let url = spawn_hello_server();
        let resp = send(RestRequest {
            method: "GET".into(),
            url,
            headers: vec![],
            body: None,
        })
        .await
        .unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, "hello-lynora");
    }

    #[test]
    fn prepare_expands_url_and_headers() {
        let doc = RequestDocument {
            id: "1".into(),
            name: "t".into(),
            method: "GET".into(),
            url: "{{base}}/x".into(),
            headers: vec![Header {
                key: "X-Token".into(),
                value: "{{token}}".into(),
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
        let mut vars = HashMap::new();
        vars.insert("base".into(), "http://example.com".into());
        vars.insert("token".into(), "abc".into());
        let req = prepare_request(&doc, &vars).unwrap();
        assert_eq!(req.url, "http://example.com/x");
        assert_eq!(req.headers, vec![("X-Token".into(), "abc".into())]);
    }

    #[test]
    fn prepare_applies_bearer_auth() {
        let doc = RequestDocument {
            id: "1".into(),
            name: "t".into(),
            method: "GET".into(),
            url: "http://example.com".into(),
            headers: vec![],
            body: None,
            protocol: Protocol::Rest,
            auth: Some(AuthConfig {
                kind: AuthKind::Bearer,
                token: Some("{{tok}}".into()),
                ..Default::default()
            }),
            graphql: None,
            grpc: None,
            expect_status: None,
            websocket: None,
            sse: None,
        };
        let mut vars = HashMap::new();
        vars.insert("tok".into(), "secret".into());
        let req = prepare_request(&doc, &vars).unwrap();
        assert_eq!(
            req.headers,
            vec![("Authorization".into(), "Bearer secret".into())]
        );
    }
}
