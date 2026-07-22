use crate::vars;
use crate::{LynoraError, Result};
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum AuthKind {
    #[default]
    None,
    Bearer,
    Basic,
    ApiKey,
    #[serde(rename = "oauth2Pkce")]
    OAuth2Pkce,
    #[serde(rename = "awsSigV4")]
    AwsSigV4,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    #[serde(default)]
    pub kind: AuthKind,
    /// Bearer token or API key value / Basic password / OAuth access token
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// API key header or query param name
    #[serde(default)]
    pub key_name: Option<String>,
    #[serde(default)]
    pub api_key_in: ApiKeyLocation,
    // OAuth2 PKCE fields
    #[serde(default)]
    pub auth_url: Option<String>,
    #[serde(default)]
    pub token_url: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    // AWS SigV4
    #[serde(default)]
    pub access_key_id: Option<String>,
    #[serde(default)]
    pub secret_access_key: Option<String>,
    #[serde(default)]
    pub session_token: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PkceChallenge {
    pub code_verifier: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
}

pub fn generate_pkce() -> PkceChallenge {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    PkceChallenge {
        code_verifier,
        code_challenge,
        code_challenge_method: "S256".into(),
    }
}

pub fn build_authorize_url(auth: &AuthConfig, pkce: &PkceChallenge, state: &str) -> Result<String> {
    let auth_url = auth
        .auth_url
        .as_deref()
        .ok_or_else(|| LynoraError::Message("OAuth2 auth_url required".into()))?;
    let client_id = auth
        .client_id
        .as_deref()
        .ok_or_else(|| LynoraError::Message("OAuth2 client_id required".into()))?;
    let redirect_uri = auth
        .redirect_uri
        .as_deref()
        .ok_or_else(|| LynoraError::Message("OAuth2 redirect_uri required".into()))?;

    let mut url = url::Url::parse(auth_url)
        .map_err(|e| LynoraError::Message(format!("invalid auth_url: {e}")))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code");
        q.append_pair("client_id", client_id);
        q.append_pair("redirect_uri", redirect_uri);
        q.append_pair("code_challenge", &pkce.code_challenge);
        q.append_pair("code_challenge_method", &pkce.code_challenge_method);
        q.append_pair("state", state);
        if let Some(scope) = &auth.scope {
            q.append_pair("scope", scope);
        }
    }
    Ok(url.into())
}

pub async fn exchange_token(
    auth: &AuthConfig,
    code: &str,
    code_verifier: &str,
) -> Result<serde_json::Value> {
    let token_url = auth
        .token_url
        .as_deref()
        .ok_or_else(|| LynoraError::Message("OAuth2 token_url required".into()))?;
    let client_id = auth
        .client_id
        .as_deref()
        .ok_or_else(|| LynoraError::Message("OAuth2 client_id required".into()))?;
    let redirect_uri = auth
        .redirect_uri
        .as_deref()
        .ok_or_else(|| LynoraError::Message("OAuth2 redirect_uri required".into()))?;

    let client = reqwest::Client::new();
    let resp = client
        .post(token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
            urlencoding(code),
            urlencoding(redirect_uri),
            urlencoding(client_id),
            urlencoding(code_verifier),
        ))
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        return Err(LynoraError::Message(format!(
            "token exchange failed ({status}): {text}"
        )));
    }
    Ok(serde_json::from_str(&text)?)
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Expand auth string fields with environment variables.
pub fn expand_auth(auth: &AuthConfig, vars: &HashMap<String, String>) -> Result<AuthConfig> {
    let expand_opt = |v: &Option<String>| -> Result<Option<String>> {
        match v {
            Some(s) => Ok(Some(vars::expand(s, vars)?)),
            None => Ok(None),
        }
    };
    Ok(AuthConfig {
        kind: auth.kind.clone(),
        token: expand_opt(&auth.token)?,
        username: expand_opt(&auth.username)?,
        password: expand_opt(&auth.password)?,
        key_name: expand_opt(&auth.key_name)?,
        api_key_in: auth.api_key_in.clone(),
        auth_url: expand_opt(&auth.auth_url)?,
        token_url: expand_opt(&auth.token_url)?,
        client_id: expand_opt(&auth.client_id)?,
        redirect_uri: expand_opt(&auth.redirect_uri)?,
        scope: expand_opt(&auth.scope)?,
        access_key_id: expand_opt(&auth.access_key_id)?,
        secret_access_key: expand_opt(&auth.secret_access_key)?,
        session_token: expand_opt(&auth.session_token)?,
        region: expand_opt(&auth.region)?,
        service: expand_opt(&auth.service)?,
    })
}

/// Apply non-SigV4 auth to headers/URL. SigV4 is applied later with method+body.
pub fn apply_auth_headers(
    mut headers: Vec<(String, String)>,
    mut url: String,
    auth: &AuthConfig,
) -> Result<(String, Vec<(String, String)>)> {
    match auth.kind {
        AuthKind::None | AuthKind::AwsSigV4 | AuthKind::OAuth2Pkce => {
            // OAuth2 uses token field once exchanged; treat like bearer if token set
            if matches!(auth.kind, AuthKind::OAuth2Pkce) {
                if let Some(token) = &auth.token {
                    upsert_header(&mut headers, "Authorization", &format!("Bearer {token}"));
                }
            }
            Ok((url, headers))
        }
        AuthKind::Bearer => {
            let token = auth
                .token
                .as_deref()
                .ok_or_else(|| LynoraError::Message("Bearer token required".into()))?;
            upsert_header(&mut headers, "Authorization", &format!("Bearer {token}"));
            Ok((url, headers))
        }
        AuthKind::Basic => {
            let user = auth.username.as_deref().unwrap_or("");
            let pass = auth.password.as_deref().unwrap_or("");
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
            upsert_header(&mut headers, "Authorization", &format!("Basic {encoded}"));
            Ok((url, headers))
        }
        AuthKind::ApiKey => {
            let name = auth
                .key_name
                .as_deref()
                .ok_or_else(|| LynoraError::Message("API key name required".into()))?;
            let value = auth
                .token
                .as_deref()
                .ok_or_else(|| LynoraError::Message("API key value required".into()))?;
            match auth.api_key_in {
                ApiKeyLocation::Header => {
                    upsert_header(&mut headers, name, value);
                    Ok((url, headers))
                }
                ApiKeyLocation::Query => {
                    let sep = if url.contains('?') { '&' } else { '?' };
                    url = format!("{url}{sep}{name}={}", urlencoding(value));
                    Ok((url, headers))
                }
            }
        }
    }
}

pub fn apply_aws_sigv4(
    method: &str,
    url: &str,
    headers: &mut Vec<(String, String)>,
    body: Option<&str>,
    auth: &AuthConfig,
) -> Result<()> {
    use aws_credential_types::Credentials;
    use aws_sigv4::http_request::{sign, SignableBody, SignableRequest, SigningSettings};
    use aws_sigv4::sign::v4::SigningParams;
    use std::time::SystemTime;

    let access_key = auth
        .access_key_id
        .as_deref()
        .ok_or_else(|| LynoraError::Message("AWS access_key_id required".into()))?;
    let secret = auth
        .secret_access_key
        .as_deref()
        .ok_or_else(|| LynoraError::Message("AWS secret_access_key required".into()))?;
    let region = auth.region.as_deref().unwrap_or("us-east-1");
    let service = auth.service.as_deref().unwrap_or("execute-api");

    let identity = Credentials::new(
        access_key,
        secret,
        auth.session_token.clone(),
        None,
        "lynora",
    );
    let identity = identity.into();

    let body_bytes = body.unwrap_or("").as_bytes();
    let header_refs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let signable = SignableRequest::new(
        method,
        url,
        header_refs.into_iter(),
        SignableBody::Bytes(body_bytes),
    )
    .map_err(|e| LynoraError::Message(format!("sigv4 signable: {e}")))?;

    let mut settings = SigningSettings::default();
    settings.expires_in = Some(Duration::from_secs(300));

    let params = SigningParams::builder()
        .identity(&identity)
        .region(region)
        .name(service)
        .time(SystemTime::now())
        .settings(settings)
        .build()
        .map_err(|e| LynoraError::Message(format!("sigv4 params: {e}")))?
        .into();

    let (instructions, _) = sign(signable, &params)
        .map_err(|e| LynoraError::Message(format!("sigv4 sign: {e}")))?
        .into_parts();

    // Apply signing instructions by building an http::Request
    let mut builder = http::Request::builder().method(method).uri(url);
    for (k, v) in headers.iter() {
        builder = builder.header(k, v);
    }
    let mut req = builder
        .body(body.unwrap_or("").to_string())
        .map_err(|e| LynoraError::Message(format!("http build: {e}")))?;
    instructions.apply_to_request_http1x(&mut req);

    headers.clear();
    for (k, v) in req.headers().iter() {
        headers.push((
            k.as_str().to_string(),
            v.to_str().unwrap_or("").to_string(),
        ));
    }
    Ok(())
}

fn upsert_header(headers: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some((_, v)) = headers
        .iter_mut()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
    {
        *v = value.to_string();
    } else {
        headers.push((key.to_string(), value.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_header() {
        let auth = AuthConfig {
            kind: AuthKind::Bearer,
            token: Some("abc".into()),
            ..Default::default()
        };
        let (url, headers) = apply_auth_headers(vec![], "http://x".into(), &auth).unwrap();
        assert_eq!(url, "http://x");
        assert_eq!(
            headers,
            vec![("Authorization".into(), "Bearer abc".into())]
        );
    }

    #[test]
    fn basic_header() {
        let auth = AuthConfig {
            kind: AuthKind::Basic,
            username: Some("u".into()),
            password: Some("p".into()),
            ..Default::default()
        };
        let (_, headers) = apply_auth_headers(vec![], "http://x".into(), &auth).unwrap();
        assert_eq!(
            headers[0].1,
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode("u:p")
            )
        );
    }

    #[test]
    fn api_key_query() {
        let auth = AuthConfig {
            kind: AuthKind::ApiKey,
            key_name: Some("key".into()),
            token: Some("secret".into()),
            api_key_in: ApiKeyLocation::Query,
            ..Default::default()
        };
        let (url, headers) = apply_auth_headers(vec![], "http://x/y".into(), &auth).unwrap();
        assert!(url.contains("?key=secret"));
        assert!(headers.is_empty());
    }

    #[test]
    fn pkce_s256_shape() {
        let pkce = generate_pkce();
        assert_eq!(pkce.code_challenge_method, "S256");
        assert!(!pkce.code_verifier.is_empty());
        assert!(!pkce.code_challenge.is_empty());
    }

    #[test]
    fn authorize_url_contains_challenge() {
        let auth = AuthConfig {
            kind: AuthKind::OAuth2Pkce,
            auth_url: Some("https://auth.example/authorize".into()),
            client_id: Some("cid".into()),
            redirect_uri: Some("http://localhost/cb".into()),
            scope: Some("openid".into()),
            ..Default::default()
        };
        let pkce = generate_pkce();
        let url = build_authorize_url(&auth, &pkce, "xyz").unwrap();
        assert!(url.contains("code_challenge="));
        assert!(url.contains("client_id=cid"));
        assert!(url.contains("scope=openid"));
    }
}
