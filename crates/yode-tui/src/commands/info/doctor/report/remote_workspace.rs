use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::commands::context::CommandContext;
use crate::commands::workspace_text::{workspace_bullets, WorkspaceText};
use yode_core::engine::EngineRuntimeState;

#[derive(Debug, Clone, Default)]
pub(super) struct RemoteWorkflowState {
    pub ssh: bool,
    pub repo: bool,
    pub provider_ready: bool,
    pub review_tools_ready: bool,
    pub remote_dir_writable: bool,
    pub browser_tools_present: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct RemoteExecutionState {
    pub remote_dir: String,
    pub capability_artifact: Option<String>,
    pub browser_state_artifact: Option<String>,
    pub execution_inventory_artifact: Option<String>,
    pub runtime_timeline_artifact: Option<String>,
    pub latest_browser_tool: Option<String>,
    pub latest_browser_outcome: Option<String>,
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

pub(super) fn build_remote_execution_state(
    project_root: &Path,
    runtime: Option<&EngineRuntimeState>,
) -> RemoteExecutionState {
    let remote_dir = project_root.join(".yode").join("remote");
    let browser_trace = runtime.and_then(|state| {
        state
            .tool_traces
            .iter()
            .find(|trace| matches!(trace.tool_name.as_str(), "web_search" | "web_fetch" | "web_browser"))
            .cloned()
    });

    RemoteExecutionState {
        remote_dir: remote_dir.display().to_string(),
        capability_artifact: latest_remote_artifact(&remote_dir, "remote-workflow-capability.json"),
        browser_state_artifact: latest_remote_artifact(&remote_dir, "browser-access-state.json"),
        execution_inventory_artifact: latest_remote_artifact(&remote_dir, "remote-execution-inventory.md"),
        runtime_timeline_artifact: std::fs::read_dir(project_root.join(".yode").join("status"))
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("runtime-timeline.md"))
            })
            .map(|path| path.display().to_string()),
        latest_browser_tool: browser_trace.as_ref().map(|trace| trace.tool_name.clone()),
        latest_browser_outcome: browser_trace.map(|trace| trace.output_preview),
    }
}

pub(super) fn write_remote_execution_state_artifact(
    project_root: &Path,
    session_id: &str,
    state: &RemoteExecutionState,
) -> Option<String> {
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-remote-execution-state.json", short_session));
    let payload = serde_json::json!({
        "remote_dir": state.remote_dir,
        "capability_artifact": state.capability_artifact,
        "browser_state_artifact": state.browser_state_artifact,
        "execution_inventory_artifact": state.execution_inventory_artifact,
        "runtime_timeline_artifact": state.runtime_timeline_artifact,
        "latest_browser_tool": state.latest_browser_tool,
        "latest_browser_outcome": state.latest_browser_outcome,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())).ok()?;
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

pub(super) fn render_remote_execution_workspace(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let payload: serde_json::Value = serde_json::from_str(&content).ok()?;
    Some(
        WorkspaceText::new("Remote execution workspace")
            .subtitle(path.display().to_string())
            .field(
                "Remote dir",
                payload
                    .get("remote_dir")
                    .and_then(|value| value.as_str())
                    .unwrap_or("none"),
            )
            .section(
                "Artifacts",
                workspace_bullets([
                    format!(
                        "capability={}",
                        payload
                            .get("capability_artifact")
                            .and_then(|value| value.as_str())
                            .unwrap_or("none")
                    ),
                    format!(
                        "browser_state={}",
                        payload
                            .get("browser_state_artifact")
                            .and_then(|value| value.as_str())
                            .unwrap_or("none")
                    ),
                    format!(
                        "inventory={}",
                        payload
                            .get("execution_inventory_artifact")
                            .and_then(|value| value.as_str())
                            .unwrap_or("none")
                    ),
                ]),
            )
            .section(
                "Latest browser outcome",
                workspace_bullets([
                    payload
                        .get("latest_browser_tool")
                        .and_then(|value| value.as_str())
                        .unwrap_or("none")
                        .to_string(),
                    payload
                        .get("latest_browser_outcome")
                        .and_then(|value| value.as_str())
                        .unwrap_or("none")
                        .to_string(),
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
        render_remote_execution_workspace,
        write_remote_execution_state_artifact, write_remote_execution_stub_inventory,
        write_remote_workflow_capability_artifact, RemoteExecutionState, RemoteWorkflowState,
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

    #[test]
    fn writes_and_renders_remote_execution_state() {
        let dir = std::env::temp_dir().join(format!("yode-remote-state-{}", uuid::Uuid::new_v4()));
        let remote = dir.join(".yode").join("remote");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&remote).unwrap();
        let state = RemoteExecutionState {
            remote_dir: remote.display().to_string(),
            capability_artifact: Some("/tmp/cap.json".to_string()),
            browser_state_artifact: Some("/tmp/browser.json".to_string()),
            execution_inventory_artifact: Some("/tmp/inv.md".to_string()),
            runtime_timeline_artifact: None,
            latest_browser_tool: Some("web_browser".to_string()),
            latest_browser_outcome: Some("navigate -> ok".to_string()),
        };
        let path = write_remote_execution_state_artifact(&dir, "session-1234", &state).unwrap();
        let rendered =
            render_remote_execution_workspace(std::path::Path::new(&path)).expect("workspace");
        assert!(rendered.contains("Remote execution workspace"));
        assert!(rendered.contains("web_browser"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn latest_remote_artifact(dir: &Path, suffix: &str) -> Option<String> {
    let mut entries = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    entries.into_iter().next().map(|path| path.display().to_string())
}
