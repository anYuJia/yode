use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use yode_core::permission::PermissionManager;
use yode_tools::registry::ToolRegistry;

use crate::provider_bootstrap::ProviderBootstrapMetrics;

use super::tooling::ToolingSetupMetrics;

fn startup_artifact_dir(project_root: &Path) -> PathBuf {
    project_root.join(".yode").join("startup")
}

fn write_startup_artifact(
    project_root: &Path,
    session_id: &str,
    kind: &str,
    ext: &str,
    body: &str,
) -> Option<String> {
    let dir = startup_artifact_dir(project_root);
    fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-{}.{}", short_session, kind, ext));
    fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

pub(crate) fn write_startup_profile_artifact(
    project_root: &Path,
    session_id: &str,
    summary: &str,
) -> Option<String> {
    write_startup_artifact(project_root, session_id, "startup-profile", "txt", summary)
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
        "mcp_tool_count": tooling.mcp_tool_count,
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
        "tooling-inventory",
        "json",
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
        "providers": all_provider_models,
    });
    write_startup_artifact(
        project_root,
        session_id,
        "provider-inventory",
        "json",
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
        "permission-policy",
        "json",
        &serde_json::to_string_pretty(&payload).ok()?,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::provider_bootstrap::ProviderBootstrapMetrics;

    #[test]
    fn writes_startup_artifacts() {
        let dir = std::env::temp_dir().join(format!(
            "yode-startup-artifacts-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
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
        let permissions = write_permission_policy_artifact(
            &dir,
            "session-1234",
            &PermissionManager::strict(),
        );

        assert!(startup.as_deref().is_some_and(|path| Path::new(path).exists()));
        assert!(tooling.as_deref().is_some_and(|path| Path::new(path).exists()));
        assert!(provider.as_deref().is_some_and(|path| Path::new(path).exists()));
        assert!(permissions.as_deref().is_some_and(|path| Path::new(path).exists()));
        std::fs::remove_dir_all(&dir).ok();
    }
}
