use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use yode_core::permission::PermissionManager;
use yode_tools::registry::ToolRegistry;

use crate::provider_bootstrap::ProviderBootstrapMetrics;

use super::tooling::ToolingSetupMetrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupArtifactKind {
    StartupProfile,
    ToolingInventory,
    ProviderInventory,
    McpStartupFailures,
    PermissionPolicy,
    BundleManifest,
}

impl StartupArtifactKind {
    fn slug(self) -> &'static str {
        match self {
            Self::StartupProfile => "startup-profile",
            Self::ToolingInventory => "tooling-inventory",
            Self::ProviderInventory => "provider-inventory",
            Self::McpStartupFailures => "mcp-startup-failures",
            Self::PermissionPolicy => "permission-policy",
            Self::BundleManifest => "startup-bundle-manifest",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::StartupProfile => "txt",
            Self::ToolingInventory
            | Self::ProviderInventory
            | Self::McpStartupFailures
            | Self::PermissionPolicy
            | Self::BundleManifest => "json",
        }
    }
}

fn startup_artifact_dir(project_root: &Path) -> PathBuf {
    project_root.join(".yode").join("startup")
}

fn short_session_id(session_id: &str) -> String {
    session_id.chars().take(8).collect::<String>()
}

fn write_startup_artifact(project_root: &Path, session_id: &str, kind: StartupArtifactKind, body: &str) -> Option<String> {
    let dir = startup_artifact_dir(project_root);
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!(
        "{}-{}.{}",
        short_session_id(session_id),
        kind.slug(),
        kind.extension()
    ));
    fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

fn startup_session_artifacts(project_root: &Path, session_id: &str) -> Vec<PathBuf> {
    let short_session = short_session_id(session_id);
    let mut artifacts = fs::read_dir(startup_artifact_dir(project_root))
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&short_session))
        })
        .collect::<Vec<_>>();
    artifacts.sort();
    artifacts
}

pub(crate) fn write_startup_profile_artifact(
    project_root: &Path,
    session_id: &str,
    summary: &str,
) -> Option<String> {
    write_startup_artifact(project_root, session_id, StartupArtifactKind::StartupProfile, summary)
}

pub(crate) fn write_tooling_inventory_artifact(
    project_root: &Path,
    session_id: &str,
    tooling: &ToolingSetupMetrics,
    tools: &ToolRegistry,
) -> Option<String> {
    let inventory = tools.inventory();
    let payload = serde_json::json!({
        "builtin_register_ms": tooling.builtin_register_ms,
        "mcp_connect_ms": tooling.mcp_connect_ms,
        "mcp_register_ms": tooling.mcp_register_ms,
        "skill_discovery_ms": tooling.skill_discovery_ms,
        "tooling_total_ms": tooling.total_ms,
        "builtin_tool_count": tooling.builtin_tool_count,
        "configured_mcp_server_count": tooling.configured_mcp_server_count,
        "connected_mcp_server_count": tooling.connected_mcp_server_count,
        "mcp_tool_count": tooling.mcp_tool_count,
        "mcp_startup_failure_count": tooling.mcp_startup_failures.len(),
        "discovered_skill_count": tooling.discovered_skill_count,
        "active_tool_count": inventory.active_count,
        "deferred_tool_count": inventory.deferred_count,
        "mcp_active_count": inventory.mcp_active_count,
        "mcp_deferred_count": inventory.mcp_deferred_count,
        "tool_search_enabled": inventory.tool_search_enabled,
        "tool_search_reason": inventory.tool_search_reason,
    });
    write_startup_artifact(
        project_root,
        session_id,
        StartupArtifactKind::ToolingInventory,
        &serde_json::to_string_pretty(&payload).ok()?,
    )
}

pub(crate) fn write_provider_inventory_artifact(
    project_root: &Path,
    session_id: &str,
    metrics: &ProviderBootstrapMetrics,
    provider_name: &str,
    model: &str,
    all_provider_models: &HashMap<String, Vec<String>>,
) -> Option<String> {
    let payload = serde_json::json!({
        "provider_name": provider_name,
        "model": model,
        "configured_registered": metrics.configured_registered,
        "env_detected_registered": metrics.env_detected_registered,
        "total_registered": metrics.total_registered,
        "capability_summary": metrics.capability_summary,
        "source_breakdown": metrics.source_breakdown,
        "provider_details": metrics.provider_details,
        "providers": all_provider_models,
    });
    write_startup_artifact(
        project_root,
        session_id,
        StartupArtifactKind::ProviderInventory,
        &serde_json::to_string_pretty(&payload).ok()?,
    )
}

pub(crate) fn write_mcp_startup_failure_artifact(
    project_root: &Path,
    session_id: &str,
    tooling: &ToolingSetupMetrics,
) -> Option<String> {
    if tooling.mcp_startup_failures.is_empty() {
        return None;
    }
    let payload = serde_json::json!({
        "configured_server_count": tooling.configured_mcp_server_count,
        "connected_server_count": tooling.connected_mcp_server_count,
        "mcp_tool_count": tooling.mcp_tool_count,
        "failure_count": tooling.mcp_startup_failures.len(),
        "tool_search_enabled": tooling.tool_search_enabled,
        "tool_search_reason": tooling.tool_search_reason,
        "failures": tooling.mcp_startup_failures,
    });
    write_startup_artifact(
        project_root,
        session_id,
        StartupArtifactKind::McpStartupFailures,
        &serde_json::to_string_pretty(&payload).ok()?,
    )
}

pub(crate) fn write_permission_policy_artifact(
    project_root: &Path,
    session_id: &str,
    permissions: &PermissionManager,
) -> Option<String> {
    let payload = serde_json::json!({
        "mode": permissions.mode().to_string(),
        "confirmable_tools": permissions.confirmable_tools(),
        "safe_readonly_shell_prefixes": permissions.safe_readonly_shell_prefixes(),
        "rules": permissions
            .rules_snapshot()
            .into_iter()
            .map(|rule| {
                serde_json::json!({
                    "source": format!("{:?}", rule.source),
                    "tool": rule.tool_name,
                    "behavior": match rule.behavior {
                        yode_core::permission::RuleBehavior::Allow => "allow",
                        yode_core::permission::RuleBehavior::Deny => "deny",
                        yode_core::permission::RuleBehavior::Ask => "ask",
                    },
                    "pattern": rule.pattern,
                })
            })
            .collect::<Vec<_>>(),
    });
    write_startup_artifact(
        project_root,
        session_id,
        StartupArtifactKind::PermissionPolicy,
        &serde_json::to_string_pretty(&payload).ok()?,
    )
}

pub(crate) fn write_startup_bundle_manifest_artifact(
    project_root: &Path,
    session_id: &str,
) -> Option<String> {
    let artifacts = startup_session_artifacts(project_root, session_id)
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_none_or(|name| !name.contains(StartupArtifactKind::BundleManifest.slug()))
        })
        .collect::<Vec<_>>();
    let payload = serde_json::json!({
        "session_id_prefix": short_session_id(session_id),
        "artifact_count": artifacts.len(),
        "artifacts": artifacts
            .into_iter()
            .map(|path| {
                serde_json::json!({
                    "kind": path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .and_then(|stem| stem.split_once('-'))
                        .map(|(_, kind)| kind)
                        .unwrap_or("unknown"),
                    "path": path.display().to_string(),
                })
            })
            .collect::<Vec<_>>(),
    });
    write_startup_artifact(
        project_root,
        session_id,
        StartupArtifactKind::BundleManifest,
        &serde_json::to_string_pretty(&payload).ok()?,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::app_bootstrap::tooling::McpStartupFailure;
    use crate::provider_bootstrap::{
        ProviderBootstrapMetrics, ProviderInventoryEntry, ProviderSourceBreakdown,
    };

    fn temp_project_dir(suffix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "yode-startup-artifacts-{}-{}",
            std::process::id(),
            suffix
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_startup_artifact_kinds_and_manifest() {
        let dir = temp_project_dir("manifest");
        let registry = ToolRegistry::new();
        registry.register(Arc::new(yode_tools::builtin::ReadFileTool));

        let startup = write_startup_profile_artifact(&dir, "session-1234", "summary");
        let tooling = write_tooling_inventory_artifact(
            &dir,
            "session-1234",
            &ToolingSetupMetrics::default(),
            &registry,
        );
        let provider = write_provider_inventory_artifact(
            &dir,
            "session-1234",
            &ProviderBootstrapMetrics {
                capability_summary: "openai:openai".to_string(),
                ..ProviderBootstrapMetrics::default()
            },
            "openai",
            "gpt",
            &HashMap::new(),
        );
        let manifest = write_startup_bundle_manifest_artifact(&dir, "session-1234");
        let permissions = write_permission_policy_artifact(
            &dir,
            "session-1234",
            &PermissionManager::strict(),
        );

        assert!(startup.as_deref().is_some_and(|path| Path::new(path).exists()));
        assert!(startup
            .as_deref()
            .is_some_and(|path| path.ends_with("startup-profile.txt")));
        assert!(tooling.as_deref().is_some_and(|path| Path::new(path).exists()));
        assert!(provider.as_deref().is_some_and(|path| Path::new(path).exists()));
        assert!(manifest.as_deref().is_some_and(|path| Path::new(path).exists()));
        assert!(permissions.as_deref().is_some_and(|path| Path::new(path).exists()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn writes_mcp_startup_failure_schema() {
        let dir = temp_project_dir("mcp-failures");
        let path = write_mcp_startup_failure_artifact(
            &dir,
            "session-1234",
            &ToolingSetupMetrics {
                configured_mcp_server_count: 3,
                connected_mcp_server_count: 1,
                mcp_tool_count: 2,
                tool_search_enabled: true,
                tool_search_reason: "enabled:test".to_string(),
                mcp_startup_failures: vec![
                    McpStartupFailure {
                        server: "filesystem".to_string(),
                        phase: "connect".to_string(),
                        message: "connection refused".to_string(),
                    },
                    McpStartupFailure {
                        server: "github".to_string(),
                        phase: "discover_tools".to_string(),
                        message: "tool listing failed".to_string(),
                    },
                ],
                ..ToolingSetupMetrics::default()
            },
        )
        .unwrap();

        let payload: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(payload["failure_count"].as_u64(), Some(2));
        assert_eq!(payload["configured_server_count"].as_u64(), Some(3));
        assert_eq!(
            payload["failures"][1]["phase"].as_str(),
            Some("discover_tools")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn writes_provider_inventory_with_source_breakdown() {
        let dir = temp_project_dir("provider");
        let path = write_provider_inventory_artifact(
            &dir,
            "session-1234",
            &ProviderBootstrapMetrics {
                configured_registered: 2,
                env_detected_registered: 1,
                total_registered: 3,
                capability_summary: "openai:openai models=2".to_string(),
                source_breakdown: ProviderSourceBreakdown {
                    configured_env_override: 1,
                    configured_inline: 1,
                    configured_fallback_env: 0,
                    env_detected: 1,
                    none_required: 0,
                    base_url_env_override: 0,
                    base_url_config_override: 1,
                    base_url_default: 2,
                },
                provider_details: vec![ProviderInventoryEntry {
                    name: "openai".to_string(),
                    format: "openai".to_string(),
                    model_count: 2,
                    registration_source: "configured".to_string(),
                    api_key_source: "env_override:OPENAI_API_KEY".to_string(),
                    base_url_source: "default".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                }],
                ..ProviderBootstrapMetrics::default()
            },
            "openai",
            "gpt-4o",
            &HashMap::from([("openai".to_string(), vec!["gpt-4o".to_string()])]),
        )
        .unwrap();

        let payload: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(payload["source_breakdown"]["configured_env_override"].as_u64(), Some(1));
        assert_eq!(
            payload["provider_details"][0]["api_key_source"].as_str(),
            Some("env_override:OPENAI_API_KEY")
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
