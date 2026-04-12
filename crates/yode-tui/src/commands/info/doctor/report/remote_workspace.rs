use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::commands::context::CommandContext;

#[derive(Debug, Clone, Default)]
pub(super) struct RemoteWorkflowState {
    pub ssh: bool,
    pub repo: bool,
    pub provider_ready: bool,
    pub review_tools_ready: bool,
    pub remote_dir_writable: bool,
    pub browser_tools_present: bool,
}

pub(super) fn build_remote_workflow_state(ctx: &CommandContext) -> RemoteWorkflowState {
    let project_root = PathBuf::from(&ctx.session.working_dir);
    let tool_names = ctx
        .tools
        .definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    let remote_dir = project_root.join(".yode").join("remote");
    let remote_dir_writable = std::fs::create_dir_all(&remote_dir)
        .and_then(|_| {
            let probe = remote_dir.join(".remote-workflow-check");
            std::fs::write(&probe, b"ok")?;
            std::fs::remove_file(probe)?;
            Ok(())
        })
        .is_ok();

    RemoteWorkflowState {
        ssh: ctx.terminal_caps.in_ssh,
        repo: project_root.join(".git").exists(),
        provider_ready: !ctx.all_provider_models.is_empty(),
        review_tools_ready: ["review_changes", "review_pipeline", "review_then_commit"]
            .into_iter()
            .all(|tool| tool_names.contains(tool)),
        remote_dir_writable,
        browser_tools_present: ["web_search", "web_fetch", "web_browser"]
            .into_iter()
            .any(|tool| tool_names.contains(tool)),
    }
}

pub(super) fn remote_missing_prereq_summary(state: &RemoteWorkflowState) -> String {
    let mut missing = Vec::new();
    if !state.provider_ready {
        missing.push("provider");
    }
    if !state.repo {
        missing.push("git repo");
    }
    if !state.review_tools_ready {
        missing.push("review tools");
    }
    if !state.remote_dir_writable {
        missing.push("remote artifact dir");
    }
    if missing.is_empty() {
        "none".to_string()
    } else {
        missing.join(", ")
    }
}

pub(super) fn browser_capability_checklist(ctx: &CommandContext) -> Vec<String> {
    let tool_names = ctx
        .tools
        .definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    ["web_search", "web_fetch", "web_browser"]
        .into_iter()
        .map(|tool| {
            if tool_names.contains(tool) {
                format!("  [ok] {} available", tool)
            } else {
                format!("  [--] {} unavailable", tool)
            }
        })
        .collect()
}

pub(super) fn remote_command_surface_inventory() -> String {
    [
        "/doctor remote",
        "/doctor remote-review",
        "/doctor remote-artifacts",
        "/doctor bundle",
    ]
    .join(", ")
}

pub(super) fn write_remote_workflow_capability_artifact(
    project_root: &Path,
    session_id: &str,
    state: &RemoteWorkflowState,
    command_inventory: &str,
) -> Option<String> {
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-remote-workflow-capability.json", short_session));
    let payload = serde_json::json!({
        "ssh": state.ssh,
        "repo": state.repo,
        "provider_ready": state.provider_ready,
        "review_tools_ready": state.review_tools_ready,
        "remote_dir_writable": state.remote_dir_writable,
        "browser_tools_present": state.browser_tools_present,
        "command_inventory": command_inventory,
        "missing_prereqs": remote_missing_prereq_summary(state),
    });
    std::fs::write(&path, serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())).ok()?;
    Some(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        remote_command_surface_inventory, remote_missing_prereq_summary,
        write_remote_workflow_capability_artifact, RemoteWorkflowState,
    };

    #[test]
    fn missing_prereq_summary_lists_missing_inputs() {
        let summary = remote_missing_prereq_summary(&RemoteWorkflowState {
            provider_ready: false,
            repo: false,
            review_tools_ready: false,
            remote_dir_writable: false,
            ..RemoteWorkflowState::default()
        });
        assert!(summary.contains("provider"));
        assert!(summary.contains("git repo"));
    }

    #[test]
    fn command_inventory_lists_remote_commands() {
        let inventory = remote_command_surface_inventory();
        assert!(inventory.contains("/doctor remote"));
        assert!(inventory.contains("/doctor bundle"));
    }

    #[test]
    fn writes_remote_capability_artifact() {
        let dir = std::env::temp_dir().join(format!("yode-remote-cap-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_remote_workflow_capability_artifact(
            &dir,
            "session-1234",
            &RemoteWorkflowState {
                provider_ready: true,
                repo: true,
                review_tools_ready: true,
                remote_dir_writable: true,
                browser_tools_present: true,
                ssh: false,
            },
            "/doctor remote",
        )
        .unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("\"provider_ready\": true"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
