use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ProviderSourceBreakdownSummary {
    pub configured_env_override: usize,
    pub configured_inline: usize,
    pub configured_fallback_env: usize,
    pub env_detected: usize,
    pub none_required: usize,
    pub base_url_env_override: usize,
    pub base_url_config_override: usize,
    pub base_url_default: usize,
}

impl ProviderSourceBreakdownSummary {
    pub(crate) fn compact_label(&self) -> String {
        format!(
            "cfg_env={} cfg_inline={} cfg_fallback={} env_detected={} none={} base_env={} base_cfg={} base_default={}",
            self.configured_env_override,
            self.configured_inline,
            self.configured_fallback_env,
            self.env_detected,
            self.none_required,
            self.base_url_env_override,
            self.base_url_config_override,
            self.base_url_default,
        )
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ProviderInventoryDetailSummary {
    pub name: String,
    pub format: String,
    pub model_count: usize,
    pub registration_source: String,
    pub api_key_source: String,
    pub base_url_source: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ProviderInventorySummary {
    pub path: PathBuf,
    pub provider_name: String,
    pub model: String,
    pub configured_registered: usize,
    pub env_detected_registered: usize,
    pub total_registered: usize,
    pub capability_summary: String,
    pub source_breakdown: ProviderSourceBreakdownSummary,
    pub provider_details: Vec<ProviderInventoryDetailSummary>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct McpStartupFailureEntry {
    pub server: String,
    pub phase: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct McpStartupFailureSummary {
    pub path: PathBuf,
    pub configured_server_count: usize,
    pub connected_server_count: usize,
    pub mcp_tool_count: usize,
    pub failure_count: usize,
    pub failures: Vec<McpStartupFailureEntry>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct StartupManifestSummary {
    pub path: PathBuf,
    pub artifact_count: usize,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct SettingsScopeEntry {
    pub scope: String,
    pub path: String,
    #[serde(default)]
    pub exists: bool,
    #[serde(default)]
    pub permission_default_mode: Option<String>,
    #[serde(default)]
    pub permission_rule_count: usize,
    #[serde(default)]
    pub mcp_server_count: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SettingsScopeSummary {
    pub path: PathBuf,
    pub scopes: Vec<SettingsScopeEntry>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ManagedMcpInventorySummary {
    pub path: PathBuf,
    pub effective_server_count: usize,
    pub configured_server_count: usize,
    pub connected_server_count: usize,
    pub mcp_tool_count: usize,
    pub failure_count: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ToolSearchActivationSummary {
    pub path: PathBuf,
    pub tool_search_enabled: bool,
    pub tool_search_reason: String,
    pub deferred_tool_count: usize,
    pub deferred_mcp_tool_count: usize,
    pub activation_count: usize,
    pub last_activated_tool: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawProviderInventorySummary {
    #[serde(default)]
    provider_name: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    configured_registered: usize,
    #[serde(default)]
    env_detected_registered: usize,
    #[serde(default)]
    total_registered: usize,
    #[serde(default)]
    capability_summary: String,
    #[serde(default)]
    source_breakdown: ProviderSourceBreakdownSummary,
    #[serde(default)]
    provider_details: Vec<ProviderInventoryDetailSummary>,
}

#[derive(Debug, Deserialize)]
struct RawMcpStartupFailureSummary {
    #[serde(default)]
    configured_server_count: usize,
    #[serde(default)]
    connected_server_count: usize,
    #[serde(default)]
    mcp_tool_count: usize,
    #[serde(default)]
    failure_count: usize,
    #[serde(default)]
    failures: Vec<McpStartupFailureEntry>,
}

#[derive(Debug, Deserialize)]
struct RawStartupManifestSummary {
    #[serde(default)]
    artifact_count: usize,
}

#[derive(Debug, Deserialize)]
struct RawSettingsScopeSummary {
    #[serde(default)]
    scopes: Vec<SettingsScopeEntry>,
}

#[derive(Debug, Deserialize)]
struct RawManagedMcpInventorySummary {
    #[serde(default)]
    effective_server_count: usize,
    #[serde(default)]
    configured_server_count: usize,
    #[serde(default)]
    connected_server_count: usize,
    #[serde(default)]
    mcp_tool_count: usize,
    #[serde(default)]
    failure_count: usize,
}

#[derive(Debug, Deserialize)]
struct RawToolSearchActivationSummary {
    #[serde(default)]
    tool_search_enabled: bool,
    #[serde(default)]
    tool_search_reason: String,
    #[serde(default)]
    deferred_tool_count: usize,
    #[serde(default)]
    deferred_mcp_tool_count: usize,
    #[serde(default)]
    activation_count: usize,
    #[serde(default)]
    last_activated_tool: Option<String>,
}

pub(crate) fn latest_startup_artifact(project_root: &Path, suffix: &str) -> Option<PathBuf> {
    let dir = project_root.join(".yode").join("startup");
    let mut entries = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(suffix))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries.into_iter().next()
}

pub(crate) fn latest_startup_artifact_link(project_root: &Path, suffix: &str) -> Option<String> {
    latest_startup_artifact(project_root, suffix).map(|path| path.display().to_string())
}

pub(crate) fn latest_provider_inventory(project_root: &Path) -> Option<ProviderInventorySummary> {
    let path = latest_startup_artifact(project_root, "provider-inventory.json")?;
    let payload: RawProviderInventorySummary =
        serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
    Some(ProviderInventorySummary {
        path,
        provider_name: payload.provider_name,
        model: payload.model,
        configured_registered: payload.configured_registered,
        env_detected_registered: payload.env_detected_registered,
        total_registered: payload.total_registered,
        capability_summary: payload.capability_summary,
        source_breakdown: payload.source_breakdown,
        provider_details: payload.provider_details,
    })
}

pub(crate) fn latest_mcp_startup_failures(project_root: &Path) -> Option<McpStartupFailureSummary> {
    let path = latest_startup_artifact(project_root, "mcp-startup-failures.json")?;
    let payload: RawMcpStartupFailureSummary =
        serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
    Some(McpStartupFailureSummary {
        path,
        configured_server_count: payload.configured_server_count,
        connected_server_count: payload.connected_server_count,
        mcp_tool_count: payload.mcp_tool_count,
        failure_count: payload.failure_count,
        failures: payload.failures,
    })
}

pub(crate) fn latest_startup_manifest(project_root: &Path) -> Option<StartupManifestSummary> {
    let path = latest_startup_artifact(project_root, "startup-bundle-manifest.json")?;
    let payload: RawStartupManifestSummary =
        serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
    Some(StartupManifestSummary {
        path,
        artifact_count: payload.artifact_count,
    })
}

pub(crate) fn latest_settings_scopes(project_root: &Path) -> Option<SettingsScopeSummary> {
    let path = latest_startup_artifact(project_root, "settings-scopes.json")?;
    let payload: RawSettingsScopeSummary =
        serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
    Some(SettingsScopeSummary {
        path,
        scopes: payload.scopes,
    })
}

pub(crate) fn latest_managed_mcp_inventory(project_root: &Path) -> Option<ManagedMcpInventorySummary> {
    let path = latest_startup_artifact(project_root, "managed-mcp-inventory.json")?;
    let payload: RawManagedMcpInventorySummary =
        serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
    Some(ManagedMcpInventorySummary {
        path,
        effective_server_count: payload.effective_server_count,
        configured_server_count: payload.configured_server_count,
        connected_server_count: payload.connected_server_count,
        mcp_tool_count: payload.mcp_tool_count,
        failure_count: payload.failure_count,
    })
}

pub(crate) fn latest_tool_search_activation(project_root: &Path) -> Option<ToolSearchActivationSummary> {
    let path = latest_startup_artifact(project_root, "tool-search-activation.json")?;
    let payload: RawToolSearchActivationSummary =
        serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
    Some(ToolSearchActivationSummary {
        path,
        tool_search_enabled: payload.tool_search_enabled,
        tool_search_reason: payload.tool_search_reason,
        deferred_tool_count: payload.deferred_tool_count,
        deferred_mcp_tool_count: payload.deferred_mcp_tool_count,
        activation_count: payload.activation_count,
        last_activated_tool: payload.last_activated_tool,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        latest_managed_mcp_inventory, latest_mcp_startup_failures, latest_provider_inventory,
        latest_settings_scopes, latest_startup_artifact_link, latest_startup_manifest,
        latest_tool_search_activation,
    };

    fn temp_project_dir(suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "yode-startup-artifacts-ui-{}-{}",
            std::process::id(),
            suffix
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yode").join("startup")).unwrap();
        dir
    }

    #[test]
    fn parses_provider_inventory_and_manifest() {
        let dir = temp_project_dir("provider");
        std::fs::write(
            dir.join(".yode").join("startup").join("session12-provider-inventory.json"),
            r#"{
  "provider_name": "openai",
  "model": "gpt-4o",
  "configured_registered": 2,
  "env_detected_registered": 1,
  "total_registered": 3,
  "capability_summary": "openai:openai models=2",
  "source_breakdown": {
    "configured_env_override": 1,
    "configured_inline": 0,
    "configured_fallback_env": 1,
    "env_detected": 1,
    "none_required": 0,
    "base_url_env_override": 0,
    "base_url_config_override": 1,
    "base_url_default": 2
  },
  "provider_details": [
    {
      "name": "openai",
      "format": "openai",
      "model_count": 2,
      "registration_source": "configured",
      "api_key_source": "env_override:OPENAI_API_KEY",
      "base_url_source": "default",
      "base_url": "https://api.openai.com/v1"
    }
  ]
}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join(".yode").join("startup").join("session12-startup-bundle-manifest.json"),
            r#"{"artifact_count": 4}"#,
        )
        .unwrap();

        let provider = latest_provider_inventory(&dir).unwrap();
        assert_eq!(provider.provider_name, "openai");
        assert_eq!(provider.source_breakdown.configured_env_override, 1);
        assert_eq!(provider.provider_details[0].api_key_source, "env_override:OPENAI_API_KEY");

        let manifest = latest_startup_manifest(&dir).unwrap();
        assert_eq!(manifest.artifact_count, 4);
        assert!(latest_startup_artifact_link(&dir, "provider-inventory.json").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_mcp_startup_failures() {
        let dir = temp_project_dir("mcp");
        std::fs::write(
            dir.join(".yode").join("startup").join("session12-mcp-startup-failures.json"),
            r#"{
  "configured_server_count": 3,
  "connected_server_count": 1,
  "mcp_tool_count": 4,
  "failure_count": 2,
  "failures": [
    {"server": "filesystem", "phase": "connect", "message": "connection refused"},
    {"server": "github", "phase": "discover_tools", "message": "tool listing failed"}
  ]
}"#,
        )
        .unwrap();

        let summary = latest_mcp_startup_failures(&dir).unwrap();
        assert_eq!(summary.failure_count, 2);
        assert_eq!(summary.failures[1].phase, "discover_tools");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_settings_scopes_and_managed_mcp_inventory() {
        let dir = temp_project_dir("settings");
        std::fs::write(
            dir.join(".yode").join("startup").join("session12-settings-scopes.json"),
            r#"{
  "scopes": [
    {
      "scope": "managed",
      "path": "/tmp/managed.toml",
      "exists": true,
      "permission_default_mode": "auto",
      "permission_rule_count": 2,
      "mcp_server_count": 1
    },
    {
      "scope": "local",
      "path": "/tmp/project/.yode/config.local.toml",
      "exists": true,
      "permission_default_mode": "accept-edits",
      "permission_rule_count": 1,
      "mcp_server_count": 0
    }
  ]
}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join(".yode").join("startup").join("session12-managed-mcp-inventory.json"),
            r#"{
  "effective_server_count": 2,
  "configured_server_count": 2,
  "connected_server_count": 1,
  "mcp_tool_count": 4,
  "failure_count": 1
}"#,
        )
        .unwrap();

        let scopes = latest_settings_scopes(&dir).unwrap();
        assert_eq!(scopes.scopes.len(), 2);
        assert_eq!(scopes.scopes[0].scope, "managed");
        let inventory = latest_managed_mcp_inventory(&dir).unwrap();
        assert_eq!(inventory.effective_server_count, 2);
        assert_eq!(inventory.mcp_tool_count, 4);
        std::fs::write(
            dir.join(".yode").join("startup").join("session12-tool-search-activation.json"),
            r#"{
  "tool_search_enabled": true,
  "tool_search_reason": "enabled:test",
  "deferred_tool_count": 5,
  "deferred_mcp_tool_count": 3,
  "activation_count": 2,
  "last_activated_tool": "mcp__github_list_prs"
}"#,
        )
        .unwrap();
        let tool_search = latest_tool_search_activation(&dir).unwrap();
        assert!(tool_search.tool_search_enabled);
        assert_eq!(tool_search.deferred_mcp_tool_count, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
