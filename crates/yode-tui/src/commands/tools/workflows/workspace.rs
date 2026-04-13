use std::path::Path;

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
    "Remote bridge: pair `/workflows preview` with `/doctor remote-review` before executing write-capable workflows in remote contexts."
}

#[cfg(test)]
mod tests {
    use super::{
        nested_workflow_guard_narrative, workflow_checkpoint_workspace, workflow_remote_bridge_hint,
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
    }
}
