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
    #[serde(default)]
    pub permissions: PermissionsConfig,
    #[serde(default)]
    pub hooks: HooksConfig,
    #[serde(default)]
    pub cost: CostConfig,
    #[serde(default)]
    pub update: UpdateConfig,
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
    /// Allowed models for this provider. Empty means unrestricted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
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
    /// Output style for AI responses: "default", "explanatory", "learning"
    #[serde(default = "default_output_style")]
    pub output_style: String,
}

fn default_output_style() -> String {
    "default".to_string()
}

// ─── Permission Config ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PermissionsConfig {
    /// Default permission mode: "default", "plan", "auto", "accept-edits", "bypass"
    #[serde(default)]
    pub default_mode: Option<String>,
    /// Rules that always allow specific tool+pattern combos
    #[serde(default)]
    pub always_allow: Vec<PermissionRuleEntry>,
    /// Rules that always deny specific tool+pattern combos
    #[serde(default)]
    pub always_deny: Vec<PermissionRuleEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PermissionRuleEntry {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ─── Hook Config ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub hooks: Vec<HookEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HookEntry {
    pub command: String,
    pub events: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_filter: Option<Vec<String>>,
    #[serde(default = "default_hook_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub can_block: bool,
}

fn default_hook_timeout() -> u64 {
    10
}

// ─── Cost Config ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CostConfig {
    /// Maximum budget in USD per session. 0 or absent means unlimited.
    #[serde(default)]
    pub max_budget_usd: Option<f64>,
    /// Whether to show cost summary after each turn
    #[serde(default)]
    pub show_cost_per_turn: bool,
}

// ─── Update Config ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateConfig {
    /// Whether to automatically check for updates on startup
    #[serde(default = "default_true")]
    pub auto_check: bool,
    /// Whether to automatically download updates in background
    #[serde(default = "default_true")]
    pub auto_download: bool,
    /// Last checked timestamp (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked: Option<String>,
    /// Last downloaded version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_downloaded_version: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            auto_check: true,
            auto_download: true,
            last_checked: None,
            last_downloaded_version: None,
        }
    }
}

// ─── Config Loading ─────────────────────────────────────────────────────────

impl Config {
    /// Load config from the default config file, merging with built-in defaults.
    pub fn load() -> Result<Self> {
        Self::load_from(None)
    }

    /// Load config from a specific path, or default locations.
    pub fn load_from(path: Option<&Path>) -> Result<Self> {
        let home_config = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".yode")
            .join("config.toml");

        let default_value: toml::Value = toml::from_str(include_str!("../../../config/default.toml"))?;

        let (config_value, should_persist_migration) = if let Some(p) = path {
            let user_value: toml::Value = toml::from_str(&std::fs::read_to_string(p)?)?;
            (merge_config_values(default_value, user_value), None)
        } else if home_config.exists() {
            let user_config_str = std::fs::read_to_string(&home_config)?;
            let user_value: toml::Value = toml::from_str(&user_config_str)?;
            let merged = merge_config_values(default_value, user_value.clone());
            let should_persist = (merged != user_value).then_some(home_config.clone());
            (merged, should_persist)
        } else {
            (default_value, None)
        };

        let config: Config = config_value.clone().try_into()?;
        if let Some(path) = should_persist_migration {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, toml::to_string_pretty(&config_value)?)?;
        }
        Ok(config)
    }

    /// Save config to the default config file path
    pub fn save(&self) -> Result<()> {
        let path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".yode")
            .join("config.toml");
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

fn merge_config_values(default: toml::Value, user: toml::Value) -> toml::Value {
    match (default, user) {
        (toml::Value::Table(mut default_table), toml::Value::Table(user_table)) => {
            for (key, user_value) in user_table {
                let merged = default_table
                    .remove(&key)
                    .map(|default_value| merge_config_values(default_value, user_value.clone()))
                    .unwrap_or(user_value);
                default_table.insert(key, merged);
            }
            toml::Value::Table(default_table)
        }
        (_, user_value) => user_value,
    }
}

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

#[cfg(test)]
mod tests {
    use super::{merge_config_values, Config};

    #[test]
    fn missing_update_section_uses_enabled_defaults() {
        let config = toml::from_str::<Config>(
            r#"
[llm]
default_provider = "openai"
default_model = "gpt-4o"

[tools]
bash_timeout = 120
require_confirmation = ["bash"]

[session]
db_path = ""

[ui]
language = "zh-CN"
theme = "dark"
"#,
        )
        .unwrap();

        assert!(config.update.auto_check);
        assert!(config.update.auto_download);
    }

    #[test]
    fn merge_config_values_preserves_user_values_and_adds_defaults() {
        let defaults = toml::toml! {
            [update]
            auto_check = true
            auto_download = true

            [ui]
            language = "zh-CN"
            theme = "dark"
        };
        let user = toml::toml! {
            [ui]
            theme = "light"
        };

        let merged = merge_config_values(toml::Value::Table(defaults), toml::Value::Table(user));
        assert_eq!(merged["ui"]["theme"].as_str(), Some("light"));
        assert_eq!(merged["ui"]["language"].as_str(), Some("zh-CN"));
        assert_eq!(merged["update"]["auto_check"].as_bool(), Some(true));
    }
}
