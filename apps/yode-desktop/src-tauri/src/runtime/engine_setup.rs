use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use yode_core::config::Config;
use yode_core::context::AgentContext;
use yode_core::permission::{PermissionManager, PermissionRule};
use yode_core::session::Session;
use yode_llm::types::Message;

use super::personalization_runtime::build_personalization_prompt;
use super::turn_permissions::configure_desktop_permissions;
use crate::protocol::PersonalizationState;
use crate::session_helpers::stored_message_to_message;

/// Resolve the workspace used for a session turn or compact operation.
pub(super) fn session_workspace_path(session: &Session, fallback: &Path) -> PathBuf {
    session
        .project_root
        .as_deref()
        .filter(|root| !root.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback.to_path_buf())
}

/// Apply active permission mode and per-session rules on top of config defaults.
pub(super) fn apply_runtime_permission_overrides(
    permissions: &mut PermissionManager,
    permission_mode: &Mutex<String>,
    session_permission_rules: &Mutex<std::collections::HashMap<String, Vec<PermissionRule>>>,
    session_id: &str,
) {
    if let Ok(active_mode_guard) = permission_mode.lock() {
        if let Ok(mode) = active_mode_guard.parse::<yode_core::permission::PermissionMode>() {
            permissions.set_mode(mode);
        }
    }
    if let Ok(rules) = session_permission_rules.lock() {
        if let Some(session_rules) = rules.get(session_id) {
            permissions.add_rules(session_rules.clone());
        }
    }
}

/// Build permissions for a desktop session using config defaults plus runtime overrides.
pub(super) fn build_session_permissions(
    config: &Config,
    workspace_path: &Path,
    permission_mode: &Mutex<String>,
    session_permission_rules: &Mutex<std::collections::HashMap<String, Vec<PermissionRule>>>,
    session_id: &str,
) -> PermissionManager {
    let mut permissions = configure_desktop_permissions(config, workspace_path);
    apply_runtime_permission_overrides(
        &mut permissions,
        permission_mode,
        session_permission_rules,
        session_id,
    );
    permissions
}

/// Build an `AgentContext` for a desktop session with personalization applied.
pub(super) fn build_desktop_agent_context(
    session: &Session,
    workspace_path: PathBuf,
    config: &Config,
    personalization: &PersonalizationState,
) -> AgentContext {
    let mut context = AgentContext::resume(
        session.id.clone(),
        workspace_path,
        session.provider.clone(),
        session.model.clone(),
    );
    context.project_memory_enabled = personalization.enable_memories
        && session
            .project_root
            .as_deref()
            .is_some_and(|root| !root.trim().is_empty());
    context.skip_tool_assisted_memory = personalization.skip_tool_chats;
    context.personalization_prompt = build_personalization_prompt(personalization);
    context.output_style = config.ui.output_style.clone();
    context
}

/// Decode stored DB messages into LLM messages for engine restore.
pub(super) fn restore_messages_from_stored(
    stored: impl IntoIterator<Item = yode_core::db::StoredMessage>,
) -> Vec<Message> {
    stored
        .into_iter()
        .filter_map(stored_message_to_message)
        .collect()
}

/// Shared MCP resource policy from config.
pub(super) fn mcp_resource_policy_from_config(
    config: &Config,
) -> yode_tools::tool::McpResourcePolicy {
    yode_tools::tool::McpResourcePolicy {
        allow: config.mcp.resource_allow.clone(),
        deny: config.mcp.resource_deny.clone(),
    }
}

/// Attach optional hook manager / MCP provider / resource policy to an engine.
pub(super) fn configure_engine_services(
    engine: &mut yode_core::engine::AgentEngine,
    hook_manager: Option<yode_core::hooks::HookManager>,
    mcp_resource_provider: Option<Arc<dyn yode_tools::tool::McpResourceProvider>>,
    config: &Config,
) {
    if let Some(hook_manager) = hook_manager {
        engine.set_hook_manager(hook_manager);
    }
    if let Some(mcp) = mcp_resource_provider {
        engine.set_mcp_resource_provider(mcp);
    }
    engine.set_mcp_resource_policy(mcp_resource_policy_from_config(config));
}
