use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub llm: LlmConfig,
    pub tools: ToolsConfig,
    pub session: SessionConfig,
    pub ui: UiConfig,
    #[serde(default)]
    pub mcp: McpConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    pub default_provider: String,
    pub default_model: String,
    pub openai: Option<OpenAiConfig>,
    pub anthropic: Option<AnthropicConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiConfig {
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicConfig {
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolsConfig {
    pub bash_timeout: u64,
    pub require_confirmation: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionConfig {
    pub db_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UiConfig {
    pub language: String,
    pub theme: String,
}

impl Config {
    /// Load config from the default config file, merging with built-in defaults.
    pub fn load() -> Result<Self> {
        Self::load_from(None)
    }

    /// Load config from a specific path, or default locations.
    pub fn load_from(path: Option<&Path>) -> Result<Self> {
        let config_str = if let Some(p) = path {
            std::fs::read_to_string(p)?
        } else {
            // Try project-local config first, then built-in default
            let local_config = Path::new("config/default.toml");
            if local_config.exists() {
                std::fs::read_to_string(local_config)?
            } else {
                include_str!("../../../config/default.toml").to_string()
            }
        };

        let config: Config = toml::from_str(&config_str)?;
        Ok(config)
    }

    /// Get the session database path, using default if not configured.
    pub fn session_db_path(&self) -> PathBuf {
        if self.session.db_path.is_empty() {
            let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            path.push(".yode");
            path.push("sessions.db");
            path
        } else {
            PathBuf::from(&self.session.db_path)
        }
    }
}

// Include default config at compile time as fallback
const _DEFAULT_CONFIG: &str = include_str!("../../../config/default.toml");

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Top-level MCP configuration.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}
