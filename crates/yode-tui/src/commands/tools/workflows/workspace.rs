use std::path::Path;

use crate::commands::artifact_nav::latest_artifact_by_suffix;
use crate::commands::workspace_text::{workspace_bullets, WorkspaceText};

pub(super) fn workflow_checkpoint_workspace(
    title: &str,
    path: &Path,
    description: &str,
    mode: &str,
    steps: Vec<String>,
) -> String {
    WorkspaceText::new(title)
        .subtitle(path.display().to_string())
        .field("Mode", mode.to_string())
        .field("Description", description.to_string())
        .section("Steps", workspace_bullets(steps))
        .render()
}

pub(super) fn nested_workflow_guard_narrative() -> &'static str {
    "Nested workflow invocations should stay explicit: inspect first with `/workflows preview`, then run the chosen workflow once the outer task scope is clear."
}

pub(super) fn workflow_remote_bridge_hint() -> &'static str {
    "Remote bridge: pair `/workflows preview` with `/doctor remote-review`, then inspect the latest remote capability artifact before executing write-capable workflows in remote contexts."
}

pub(super) fn workflow_jump_targets(name: &str) -> Vec<String> {
    vec![
        format!("/workflows show {}", name),
        format!("/workflows preview {}", name),
        format!("/workflows run {}", name),
        format!("/workflows run-write {}", name),
        "/inspect workflows latest".to_string(),
        "/inspect workflows timeline".to_string(),
    ]
}

pub(super) fn workflow_remote_bridge_follow_up(project_root: &Path) -> Vec<String> {
    let remote_dir = project_root.join(".yode").join("remote");
    let Some(path) = latest_artifact_by_suffix(&remote_dir, "remote-workflow-capability.json") else {
        return vec![
            "status: unknown".to_string(),
            "artifact: none".to_string(),
            "follow-up: /doctor remote-review".to_string(),
        ];
    };

    let payload = std::fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .unwrap_or_default();
    let missing = payload
        .get("missing_prereqs")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    vec![
        format!(
            "status: {}",
            if missing == "none" {
                "ready".to_string()
            } else {
                format!("missing {}", missing)
            }
        ),
        format!("artifact: {}", path.display()),
        "follow-up: /doctor remote-review".to_string(),
    ]
}

pub(super) fn write_workflow_execution_artifact(
    project_root: &Path,
    session_id: &str,
    name: &str,
    definition_path: &Path,
    description: &str,
    mode: &str,
    prompt: &str,
    steps: &[String],
) -> Option<String> {
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let slug = workflow_artifact_slug(name);
    let path = dir.join(format!(
        "{}-{}-workflow-execution.md",
        short_session, slug
    ));
    let remote_bridge = workflow_remote_bridge_follow_up(project_root)
        .into_iter()
        .map(|line| format!("- {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    let rendered_steps = steps
        .iter()
        .map(|step| format!("- {}", step))
        .collect::<Vec<_>>()
        .join("\n");
    let jump_targets = workflow_jump_targets(name)
        .into_iter()
        .map(|target| format!("- {}", target))
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!(
        "# Workflow Execution\n\n- Name: {}\n- Mode: {}\n- Definition: {}\n- Description: {}\n- Prompt: {}\n\nSteps:\n{}\n\nRemote bridge:\n{}\n\nJump targets:\n{}\n",
        name,
        mode,
        definition_path.display(),
        description,
        prompt,
        rendered_steps,
        remote_bridge,
        jump_targets,
    );
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

fn workflow_artifact_slug(name: &str) -> String {
    let slug = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "workflow".to_string()
    } else {
        slug.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        nested_workflow_guard_narrative, workflow_checkpoint_workspace,
        workflow_jump_targets, workflow_remote_bridge_follow_up, workflow_remote_bridge_hint,
        write_workflow_execution_artifact,
    };

    #[test]
    fn workflow_workspace_renders_sections() {
        let output = workflow_checkpoint_workspace(
            "Workflow preview",
            std::path::Path::new("/tmp/demo.json"),
            "demo",
            "safe",
            vec!["1. review_changes".to_string()],
        );
        assert!(output.contains("Workflow preview"));
        assert!(output.contains("Steps"));
    }

    #[test]
    fn workflow_guard_and_bridge_hints_render() {
        assert!(nested_workflow_guard_narrative().contains("Nested workflow"));
        assert!(workflow_remote_bridge_hint().contains("/doctor remote-review"));
        assert!(workflow_jump_targets("demo")[0].contains("/workflows show demo"));
    }

    #[test]
    fn remote_bridge_follow_up_reads_capability_artifact() {
        let dir = std::env::temp_dir().join(format!("yode-workflow-bridge-{}", uuid::Uuid::new_v4()));
        let remote = dir.join(".yode").join("remote");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::write(
            remote.join("session-remote-workflow-capability.json"),
            r#"{"missing_prereqs":"none"}"#,
        )
        .unwrap();
        let lines = workflow_remote_bridge_follow_up(&dir);
        assert!(lines[0].contains("ready"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_workflow_execution_artifact() {
        let dir = std::env::temp_dir().join(format!("yode-workflow-artifact-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let definition = dir.join("demo.json");
        std::fs::write(&definition, "{}").unwrap();
        let path = write_workflow_execution_artifact(
            &dir,
            "session-1234",
            "demo",
            &definition,
            "desc",
            "safe read-only",
            "Use workflow_run",
            &["1. review_changes [safe]".to_string()],
        )
        .unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("# Workflow Execution"));
        assert!(content.contains("Jump targets:"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
