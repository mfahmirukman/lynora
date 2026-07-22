use crate::rest::RestResponse;
use crate::vars;
use crate::{LynoraError, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketBody {
    /// Optional first message to send after connect.
    #[serde(default)]
    pub message: Option<String>,
    /// Stop after this many inbound text/binary messages (default 5).
    #[serde(default = "default_max_messages")]
    pub max_messages: usize,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SseBody {
    #[serde(default = "default_max_events")]
    pub max_events: usize,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_max_messages() -> usize {
    5
}
fn default_max_events() -> usize {
    10
}
fn default_timeout_ms() -> u64 {
    5_000
}

#[derive(Debug, Clone)]
pub struct RealtimeRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

pub async fn connect_websocket(
    req: RealtimeRequest,
    body: &WebSocketBody,
    vars: &HashMap<String, String>,
) -> Result<RestResponse> {
    let url = vars::expand(&req.url, vars)?;
    let message = match &body.message {
        Some(m) => Some(vars::expand(m, vars)?),
        None => None,
    };

    let started = Instant::now();
    let (ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| LynoraError::Message(format!("websocket connect failed: {e}")))?;
    let (mut write, mut read) = ws.split();

    if let Some(msg) = message {
        write
            .send(Message::Text(msg.into()))
            .await
            .map_err(|e| LynoraError::Message(format!("websocket send failed: {e}")))?;
    }

    let timeout = Duration::from_millis(body.timeout_ms.max(1));
    let mut messages = Vec::new();
    let deadline = Instant::now() + timeout;

    while messages.len() < body.max_messages.max(1) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, read.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => messages.push(t.to_string()),
            Ok(Some(Ok(Message::Binary(b)))) => {
                messages.push(format!("<binary {} bytes>", b.len()));
            }
            Ok(Some(Ok(Message::Ping(p)))) => {
                let _ = write.send(Message::Pong(p)).await;
            }
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) => break,
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(e))) => {
                return Err(LynoraError::Message(format!("websocket read failed: {e}")));
            }
            Err(_) => break,
        }
    }

    let _ = write.close().await;
    Ok(RestResponse {
        status: 101,
        headers: vec![("upgrade".into(), "websocket".into())],
        body: serde_json::to_string_pretty(&messages)?,
        duration_ms: started.elapsed().as_millis(),
    })
}

pub async fn listen_sse(
    req: RealtimeRequest,
    body: &SseBody,
    vars: &HashMap<String, String>,
) -> Result<RestResponse> {
    let url = vars::expand(&req.url, vars)?;
    let client = reqwest::Client::new();
    let mut builder = client.get(&url).header("Accept", "text/event-stream");
    for (k, v) in &req.headers {
        builder = builder.header(k, vars::expand(v, vars)?);
    }

    let started = Instant::now();
    let response = builder
        .send()
        .await
        .map_err(|e| LynoraError::Message(format!("SSE connect failed: {e}")))?;
    let status = response.status().as_u16();
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let timeout = Duration::from_millis(body.timeout_ms.max(1));
    let mut stream = response.bytes_stream();
    let mut buf = String::new();
    let mut events = Vec::new();
    let deadline = Instant::now() + timeout;

    while events.len() < body.max_events.max(1) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, stream.next()).await {
            Ok(Some(Ok(chunk))) => {
                buf.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(idx) = buf.find("\n\n") {
                    let raw = buf[..idx].to_string();
                    buf = buf[idx + 2..].to_string();
                    if let Some(event) = parse_sse_event(&raw) {
                        events.push(event);
                        if events.len() >= body.max_events.max(1) {
                            break;
                        }
                    }
                }
            }
            Ok(Some(Err(e))) => {
                return Err(LynoraError::Message(format!("SSE stream failed: {e}")));
            }
            Ok(None) | Err(_) => break,
        }
    }

    Ok(RestResponse {
        status,
        headers,
        body: serde_json::to_string_pretty(&events)?,
        duration_ms: started.elapsed().as_millis(),
    })
}

fn parse_sse_event(raw: &str) -> Option<serde_json::Value> {
    let mut event = "message".to_string();
    let mut data_lines = Vec::new();
    let mut id = None;
    for line in raw.lines() {
        if line.starts_with(':') || line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            event = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        } else if let Some(rest) = line.strip_prefix("id:") {
            id = Some(rest.trim().to_string());
        }
    }
    if data_lines.is_empty() && id.is_none() && event == "message" {
        return None;
    }
    Some(serde_json::json!({
        "event": event,
        "data": data_lines.join("\n"),
        "id": id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sse_block() {
        let ev = parse_sse_event("event: ping\ndata: hello\nid: 1\n").unwrap();
        assert_eq!(ev["event"], "ping");
        assert_eq!(ev["data"], "hello");
        assert_eq!(ev["id"], "1");
    }
}
