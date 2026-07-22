use crate::rest::RestRequest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CodeLanguage {
    Curl,
    Fetch,
    Reqwest,
}

pub fn generate(lang: CodeLanguage, req: &RestRequest) -> String {
    match lang {
        CodeLanguage::Curl => curl(req),
        CodeLanguage::Fetch => fetch(req),
        CodeLanguage::Reqwest => reqwest_snippet(req),
    }
}

fn escape_single(s: &str) -> String {
    s.replace('\'', "'\\''")
}

fn curl(req: &RestRequest) -> String {
    let mut parts = vec![format!("curl -X {} '{}'", req.method, escape_single(&req.url))];
    for (k, v) in &req.headers {
        parts.push(format!("  -H '{}: {}'", escape_single(k), escape_single(v)));
    }
    if let Some(body) = &req.body {
        parts.push(format!("  --data-raw '{}'", escape_single(body)));
    }
    parts.join(" \\\n")
}

fn fetch(req: &RestRequest) -> String {
    let mut headers_obj = String::from("{\n");
    for (i, (k, v)) in req.headers.iter().enumerate() {
        if i > 0 {
            headers_obj.push_str(",\n");
        }
        headers_obj.push_str(&format!(
            "    {}: {}",
            serde_json::to_string(k).unwrap(),
            serde_json::to_string(v).unwrap()
        ));
    }
    headers_obj.push_str("\n  }");

    let body_line = match &req.body {
        Some(b) => format!(",\n  body: {}", serde_json::to_string(b).unwrap()),
        None => String::new(),
    };

    format!(
        "fetch({}, {{\n  method: {},\n  headers: {}{}\n}});",
        serde_json::to_string(&req.url).unwrap(),
        serde_json::to_string(&req.method).unwrap(),
        headers_obj,
        body_line
    )
}

fn reqwest_snippet(req: &RestRequest) -> String {
    let mut lines = vec![
        "use reqwest::Client;".into(),
        String::new(),
        "async fn send() -> reqwest::Result<()> {".into(),
        "    let client = Client::new();".into(),
        format!(
            "    let resp = client.request(reqwest::Method::{}, {})",
            req.method.to_uppercase(),
            serde_json::to_string(&req.url).unwrap()
        ),
    ];
    for (k, v) in &req.headers {
        lines.push(format!(
            "        .header({}, {})",
            serde_json::to_string(k).unwrap(),
            serde_json::to_string(v).unwrap()
        ));
    }
    if let Some(body) = &req.body {
        lines.push(format!("        .body({})", serde_json::to_string(body).unwrap()));
    }
    lines.push("        .send()".into());
    lines.push("        .await?;".into());
    lines.push("    println!(\"{}\", resp.status());".into());
    lines.push("    Ok(())".into());
    lines.push("}".into());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RestRequest {
        RestRequest {
            method: "POST".into(),
            url: "https://api.example.com/v1".into(),
            headers: vec![
                ("Accept".into(), "application/json".into()),
                ("Authorization".into(), "Bearer t".into()),
            ],
            body: Some(r#"{"a":1}"#.into()),
        }
    }

    #[test]
    fn curl_contains_method_and_auth() {
        let out = generate(CodeLanguage::Curl, &sample());
        assert!(out.contains("-X POST"));
        assert!(out.contains("Authorization: Bearer t"));
        assert!(out.contains("--data-raw"));
    }

    #[test]
    fn fetch_is_validish_js() {
        let out = generate(CodeLanguage::Fetch, &sample());
        assert!(out.starts_with("fetch("));
        assert!(out.contains("method: \"POST\""));
    }

    #[test]
    fn reqwest_mentions_client() {
        let out = generate(CodeLanguage::Reqwest, &sample());
        assert!(out.contains("Client::new()"));
        assert!(out.contains("Method::POST"));
    }
}
