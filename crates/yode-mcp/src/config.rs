use std::collections::HashMap;

use serde::Deserialize;

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct McpServerConfig {
    /// Transport to use. Stdio and streamable HTTP/SSE are executable; websocket is parsed for diagnostics.
    #[serde(default)]
    pub transport: McpTransportConfig,
    /// Command to execute (e.g., "npx", "node", "python")
    #[serde(default)]
    pub command: String,
    /// Arguments for the command
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables to set
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Remote MCP endpoint for sse/http/websocket transports.
    #[serde(default)]
    pub url: Option<String>,
    /// Optional auth metadata for remote MCP transports.
    #[serde(default)]
    pub auth: Option<McpAuthConfig>,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum McpTransportConfig {
    #[default]
    Stdio,
    Sse,
    Http,
    Websocket,
}

impl McpTransportConfig {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Sse => "sse",
            Self::Http => "http",
            Self::Websocket => "websocket",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct McpAuthConfig {
    #[serde(default)]
    pub oauth: Option<McpOAuthConfig>,
    #[serde(default)]
    pub bearer_token_env: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct McpOAuthConfig {
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub authorization_url: Option<String>,
    #[serde(default)]
    pub token_url: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Top-level MCP configuration.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct McpConfig {
    /// Named MCP servers
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}
