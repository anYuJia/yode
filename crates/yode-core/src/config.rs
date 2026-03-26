use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub llm: LlmConfig,
    pub tools: ToolsConfig,
    pub session: SessionConfig,
    pub ui: UiConfig,
    #[serde(default)]
    pub mcp: McpConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmConfig {
    pub default_provider: String,
    pub default_model: String,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderConfig {
    pub format: String, // "openai" or "anthropic"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolsConfig {
    pub bash_timeout: u64,
    pub require_confirmation: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionConfig {
    pub db_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
        let home_config = dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".yode").join("config.toml");
        
        let config_str = if let Some(p) = path {
            std::fs::read_to_string(p)?
        } else if home_config.exists() {
            std::fs::read_to_string(home_config)?
        } else {
            include_str!("../../../config/default.toml").to_string()
        };

        let config: Config = toml::from_str(&config_str)?;
        Ok(config)
    }

    /// Save config to the default config file path
    pub fn save(&self) -> Result<()> {
        let path = dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".yode").join("config.toml");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)?;
        fs::write(path, toml_str)?;
        Ok(())
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

/// Top-level MCP configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct McpConfig {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub servers: HashMap<String, McpServerConfig>,
}
