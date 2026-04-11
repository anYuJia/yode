pub(super) fn latest_workflow_name(dir: &std::path::Path) -> Option<String> {
    let mut entries = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries.into_iter().next().and_then(|path| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| stem.to_string())
    })
}

pub(super) fn load_workflow_definition(
    dir: &std::path::Path,
    name: &str,
) -> Result<
    (
        std::path::PathBuf,
        serde_json::Value,
        Vec<serde_json::Value>,
    ),
    String,
> {
    let path = dir.join(format!("{}.json", name));
    let content = std::fs::read_to_string(&path)
        .map_err(|err| format!("Failed to read {}: {}", path.display(), err))?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|err| format!("Invalid workflow JSON {}: {}", path.display(), err))?;
    let steps = json
        .get("steps")
        .and_then(|value| value.as_array())
        .cloned()
        .ok_or_else(|| format!("Workflow {} has no steps array.", path.display()))?;
    Ok((path, json, steps))
}

pub(super) fn compact_json_preview(value: &serde_json::Value) -> String {
    let raw = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    if raw.chars().count() > 120 {
        format!("{}...", raw.chars().take(120).collect::<String>())
    } else {
        raw
    }
}

pub(super) fn workflow_template_names() -> Vec<&'static str> {
    vec![
        "review-pipeline",
        "review-then-commit",
        "ship-pipeline",
        "coordinator-review",
    ]
}

pub(super) fn workflow_template(name: &str) -> Option<(&'static str, &'static str)> {
    match name {
        "review-pipeline" => Some((
            "review-pipeline.json",
            r#"{
  "name": "review-pipeline",
  "description": "Plan a review and verification flow before shipping",
  "steps": [
    {
      "tool_name": "review_changes",
      "params": {
        "focus": "${focus}"
      }
    },
    {
      "tool_name": "verification_agent",
      "params": {
        "goal": "verify the current implementation is correct",
        "focus": "${focus}"
      }
    }
  ]
}"#,
        )),
        "review-then-commit" => Some((
            "review-then-commit.json",
            r#"{
  "name": "review-then-commit",
  "description": "Review current changes and commit only when the review is clean",
  "steps": [
    {
      "tool_name": "review_then_commit",
      "params": {
        "message": "${message}",
        "focus": "${focus}",
        "files": []
      }
    }
  ]
}"#,
        )),
        "ship-pipeline" => Some((
            "ship-pipeline.json",
            r#"{
  "name": "ship-pipeline",
  "description": "Run review, verification, and commit only when checks are clean",
  "steps": [
    {
      "tool_name": "review_pipeline",
      "params": {
        "focus": "${focus}",
        "verification_goal": "verify the current implementation is correct",
        "commit_message": "${commit_message}",
        "files": []
      }
    }
  ]
}"#,
        )),
        "coordinator-review" => Some((
            "coordinator-review.json",
            r#"{
  "name": "coordinator-review",
  "description": "Coordinate review and verification workstreams",
  "steps": [
    {
      "tool_name": "coordinate_agents",
      "params": {
        "goal": "${goal}",
        "workstreams": [
          {
            "id": "review",
            "description": "review changes",
            "prompt": "review the current workspace changes and report findings first",
            "run_in_background": false
          },
          {
            "id": "verify",
            "description": "verify behavior",
            "prompt": "verify the implementation and highlight regressions or missing tests",
            "depends_on": ["review"],
            "run_in_background": false
          }
        ]
      }
    }
  ]
}"#,
        )),
        _ => None,
    }
}

pub(super) fn workflow_requires_write_mode(steps: &[serde_json::Value]) -> bool {
    steps.iter().any(|step| {
        step.get("tool_name")
            .and_then(|value| value.as_str())
            .map(|tool_name| !is_safe_workflow_step(tool_name))
            .unwrap_or(true)
    })
}

pub(super) fn is_safe_workflow_step(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "task_output"
            | "read_file"
            | "glob"
            | "grep"
            | "ls"
            | "git_status"
            | "git_diff"
            | "git_log"
            | "project_map"
            | "memory"
            | "review_changes"
            | "verification_agent"
            | "coordinate_agents"
    )
}
