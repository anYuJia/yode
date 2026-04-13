use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::commands::context::CommandContext;
use crate::commands::workspace_text::{workspace_bullets, WorkspaceText};

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

pub(super) fn write_remote_execution_stub_inventory(
    project_root: &Path,
    session_id: &str,
) -> Option<String> {
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-remote-execution-inventory.md", short_session));
    let mut files = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    files.sort();
    let body = format!(
        "# Remote Execution Stub Inventory\n\n{}\n",
        files
            .iter()
            .map(|path| format!("- {}", path.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

pub(super) fn render_remote_capability_workspace(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let payload: serde_json::Value = serde_json::from_str(&content).ok()?;
    Some(
        WorkspaceText::new("Remote capability workspace")
            .subtitle(path.display().to_string())
            .field(
                "Missing prereqs",
                payload
                    .get("missing_prereqs")
                    .and_then(|value| value.as_str())
                    .unwrap_or("none"),
            )
            .section(
                "Capabilities",
                workspace_bullets([
                    format!(
                        "ssh={}",
                        payload.get("ssh").and_then(|value| value.as_bool()).unwrap_or(false)
                    ),
                    format!(
                        "provider_ready={}",
                        payload
                            .get("provider_ready")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false)
                    ),
                    format!(
                        "review_tools_ready={}",
                        payload
                            .get("review_tools_ready")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false)
                    ),
                ]),
            )
            .render(),
    )
}

pub(super) fn remote_prereq_severity_banner(state: &RemoteWorkflowState) -> String {
    let missing = remote_missing_prereq_summary(state);
    if missing == "none" {
        "remote prereqs: ready".to_string()
    } else {
        format!("remote prereqs: missing {}", missing)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        remote_command_surface_inventory, remote_missing_prereq_summary,
        write_remote_execution_stub_inventory, write_remote_workflow_capability_artifact,
        RemoteWorkflowState,
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

    #[test]
    fn writes_remote_execution_inventory() {
        let dir = std::env::temp_dir().join(format!("yode-remote-exec-{}", uuid::Uuid::new_v4()));
        let remote = dir.join(".yode").join("remote");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::write(remote.join("artifact.txt"), "x").unwrap();
        let path = write_remote_execution_stub_inventory(&dir, "session-1234").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("# Remote Execution Stub Inventory"));
        assert!(content.contains("artifact.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
