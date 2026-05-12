use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct McpAuthTool;

const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:8765/callback";

#[derive(Debug, Deserialize, Default)]
struct LocalConfig {
    #[serde(default)]
    mcp: LocalMcpConfig,
}

#[derive(Debug, Deserialize, Default)]
struct LocalMcpConfig {
    #[serde(default)]
    servers: HashMap<String, LocalMcpServer>,
}

#[derive(Debug, Deserialize, Default)]
struct LocalMcpServer {
    #[serde(default)]
    auth: Option<LocalMcpAuth>,
}

#[derive(Debug, Deserialize, Default)]
struct LocalMcpAuth {
    #[serde(default)]
    oauth: Option<LocalMcpOAuth>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct LocalMcpOAuth {
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    authorization_url: Option<String>,
    #[serde(default)]
    token_url: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct McpOAuthSession {
    server: String,
    client_id: String,
    authorization_url: String,
    token_url: String,
    redirect_uri: String,
    scopes: Vec<String>,
    state: String,
    code_verifier: String,
    started_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct McpOAuthToken {
    access_token: String,
    token_type: Option<String>,
    refresh_token: Option<String>,
    scope: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    saved_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[async_trait]
impl Tool for McpAuthTool {
    fn name(&self) -> &str {
        "mcp_auth"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["McpAuth".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, params: &Value) -> String {
        let server = params
            .get("server")
            .and_then(|v| v.as_str())
            .unwrap_or("server");
        format!("Authenticating MCP server: {}", server)
    }

    fn description(&self) -> &str {
        "Start or complete the OAuth authorization flow for an MCP server that requires it."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "The name of the MCP server to authenticate"
                },
                "code": {
                    "type": "string",
                    "description": "OAuth authorization code returned by the provider. Omit to start a new authorization flow."
                },
                "redirect_uri": {
                    "type": "string",
                    "description": "Redirect URI registered with the OAuth provider. Defaults to http://127.0.0.1:8765/callback."
                }
            },
            "required": ["server"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let server = params
            .get("server")
            .and_then(|v| v.as_str())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("'server' parameter is required"))?;
        let redirect_uri = params
            .get("redirect_uri")
            .and_then(|v| v.as_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(DEFAULT_REDIRECT_URI);

        if let Some(code) = params
            .get("code")
            .and_then(|v| v.as_str())
            .filter(|value| !value.trim().is_empty())
        {
            complete_oauth_flow(server, code, redirect_uri).await
        } else {
            start_oauth_flow(server, redirect_uri)
        }
    }
}

fn start_oauth_flow(server: &str, redirect_uri: &str) -> Result<ToolResult> {
    let oauth = configured_oauth(server)?;
    let client_id = required_field(oauth.client_id.as_deref(), "client_id")?;
    let authorization_url =
        required_field(oauth.authorization_url.as_deref(), "authorization_url")?;
    let token_url = required_field(oauth.token_url.as_deref(), "token_url")?;
    let state = Uuid::new_v4().to_string();
    let code_verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let code_challenge = pkce_challenge(&code_verifier);

    let auth_url = build_authorization_url(
        authorization_url,
        client_id,
        redirect_uri,
        &oauth.scopes,
        &state,
        &code_challenge,
    );
    let session = McpOAuthSession {
        server: server.to_string(),
        client_id: client_id.to_string(),
        authorization_url: authorization_url.to_string(),
        token_url: token_url.to_string(),
        redirect_uri: redirect_uri.to_string(),
        scopes: oauth.scopes,
        state,
        code_verifier,
        started_at: Utc::now(),
    };
    write_json(&oauth_session_path(server)?, &session)?;

    Ok(ToolResult::success(format!(
        "MCP OAuth authorization started for '{}'.\n\nOpen this URL in your browser:\n\n{}\n\nAfter approving access, run `mcp_auth` again with the returned `code` for server '{}'.",
        server, auth_url, server
    )))
}

async fn complete_oauth_flow(server: &str, code: &str, redirect_uri: &str) -> Result<ToolResult> {
    let session: McpOAuthSession = read_json(&oauth_session_path(server)?)?;
    if session.redirect_uri != redirect_uri {
        anyhow::bail!(
            "redirect_uri does not match the active OAuth session for '{}'",
            server
        );
    }

    let response = reqwest::Client::new()
        .post(&session.token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", session.client_id.as_str()),
            ("redirect_uri", session.redirect_uri.as_str()),
            ("code_verifier", session.code_verifier.as_str()),
        ])
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        anyhow::bail!(
            "OAuth token exchange failed for '{}': HTTP {}: {}",
            server,
            status,
            body
        );
    }
    let token_response: TokenResponse = serde_json::from_str(&body)?;
    let expires_at = token_response
        .expires_in
        .map(|seconds| Utc::now() + Duration::seconds(seconds));
    let token = McpOAuthToken {
        access_token: token_response.access_token,
        token_type: token_response.token_type,
        refresh_token: token_response.refresh_token,
        scope: token_response.scope,
        expires_at,
        saved_at: Utc::now(),
    };
    write_json(&oauth_token_path(server)?, &token)?;

    Ok(ToolResult::success(format!(
        "MCP OAuth token saved for '{}'. Restart or reconnect the MCP server for the token to be used.",
        server
    )))
}

fn configured_oauth(server: &str) -> Result<LocalMcpOAuth> {
    let config_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".yode")
        .join("config.toml");
    let contents = fs::read_to_string(&config_path).map_err(|err| {
        anyhow::anyhow!(
            "failed to read MCP config at {}: {}",
            config_path.display(),
            err
        )
    })?;
    let config: LocalConfig = toml::from_str(&contents)?;
    config
        .mcp
        .servers
        .get(server)
        .and_then(|server| server.auth.as_ref())
        .and_then(|auth| auth.oauth.clone())
        .ok_or_else(|| anyhow::anyhow!("MCP server '{}' has no OAuth config", server))
}

fn required_field<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("MCP OAuth config is missing {}", name))
}

fn build_authorization_url(
    authorization_url: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    state: &str,
    code_challenge: &str,
) -> String {
    let separator = if authorization_url.contains('?') {
        '&'
    } else {
        '?'
    };
    format!(
        "{}{}response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        authorization_url,
        separator,
        percent_encode(client_id),
        percent_encode(redirect_uri),
        percent_encode(&scopes.join(" ")),
        percent_encode(state),
        percent_encode(code_challenge)
    )
}

fn pkce_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    base64_url_no_pad(&digest)
}

fn base64_url_no_pad(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        }
    }
    out
}

fn percent_encode(input: &str) -> String {
    input
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{:02X}", byte).chars().collect(),
        })
        .collect()
}

fn oauth_dir() -> Result<PathBuf> {
    let path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".yode")
        .join("mcp-auth");
    fs::create_dir_all(&path)?;
    Ok(path)
}

fn oauth_session_path(server: &str) -> Result<PathBuf> {
    Ok(oauth_dir()?.join(format!("{}.session.json", sanitize_server_name(server))))
}

fn oauth_token_path(server: &str) -> Result<PathBuf> {
    Ok(oauth_dir()?.join(format!("{}.token.json", sanitize_server_name(server))))
}

fn sanitize_server_name(server: &str) -> String {
    server
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let contents = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&contents)?)
}

#[cfg(test)]
mod tests {
    use crate::tool::Tool;

    use super::{base64_url_no_pad, build_authorization_url, percent_encode, McpAuthTool};

    #[test]
    fn mcp_auth_requires_confirmation_for_external_auth_flow() {
        let caps = McpAuthTool.capabilities();
        assert!(caps.requires_confirmation);
        assert!(!caps.supports_auto_execution);
        assert!(!caps.read_only);
    }

    #[test]
    fn authorization_url_uses_pkce_and_encodes_query_values() {
        let rendered = build_authorization_url(
            "https://auth.example/authorize",
            "client id",
            "http://127.0.0.1:8765/callback",
            &["read:tools".to_string(), "write tools".to_string()],
            "state value",
            "challenge+value",
        );

        assert!(rendered.contains("response_type=code"));
        assert!(rendered.contains("client_id=client%20id"));
        assert!(rendered.contains("scope=read%3Atools%20write%20tools"));
        assert!(rendered.contains("state=state%20value"));
        assert!(rendered.contains("code_challenge=challenge%2Bvalue"));
    }

    #[test]
    fn url_helpers_match_oauth_safe_encoding() {
        assert_eq!(percent_encode("a b+c:/"), "a%20b%2Bc%3A%2F");
        assert_eq!(
            base64_url_no_pad(b"any carnal pleasure."),
            "YW55IGNhcm5hbCBwbGVhc3VyZS4"
        );
    }
}
