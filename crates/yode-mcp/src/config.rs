use std::collections::HashMap;

use serde::Deserialize;

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    /// Command to execute (e.g., "npx", "node", "python")
    pub command: String,
    /// Arguments for the command
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables to set
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Top-level MCP configuration.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct McpConfig {
    /// Named MCP servers
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}
