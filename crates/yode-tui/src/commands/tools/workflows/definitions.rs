use std::path::{Path, PathBuf};

pub(super) fn latest_workflow_name(dir: &Path) -> Option<String> {
    let mut entries = workflow_definition_paths(dir);
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries.into_iter().next().and_then(|path| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| stem.to_string())
    })
}

pub(super) fn load_workflow_definition(
    dir: &Path,
    name: &str,
) -> Result<(PathBuf, serde_json::Value, Vec<serde_json::Value>), String> {
    let path = workflow_definition_paths(dir)
        .into_iter()
        .find(|path| path.file_stem().and_then(|stem| stem.to_str()) == Some(name))
        .unwrap_or_else(|| dir.join(format!("{}.json", name)));
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

pub(super) fn workflow_definition_paths(dir: &Path) -> Vec<PathBuf> {
    let mut paths = workflow_definition_dirs(dir)
        .into_iter()
        .flat_map(|dir| {
            std::fs::read_dir(dir)
                .ok()
                .into_iter()
                .flat_map(|entries| entries.filter_map(Result::ok))
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    paths.extend(plugin_workflow_paths(dir));
    paths.sort();
    paths.dedup();
    paths
}

fn workflow_definition_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![dir.to_path_buf()];

    if dir.file_name().and_then(|name| name.to_str()) == Some("workflows") {
        if let Some(parent) = dir.parent() {
            if parent.file_name().and_then(|name| name.to_str()) == Some(".yode") {
                if let Some(project_root) = parent.parent() {
                    dirs.push(project_root.join(".claude").join("workflows"));
                }
            } else if parent.file_name().and_then(|name| name.to_str()) == Some(".claude") {
                if let Some(project_root) = parent.parent() {
                    dirs.push(project_root.join(".yode").join("workflows"));
                }
            }
        }
    }

    dirs.sort();
    dirs.dedup();
    dirs
}

fn plugin_workflow_paths(dir: &Path) -> Vec<PathBuf> {
    let Some(project_root) = workflow_project_root(dir) else {
        return Vec::new();
    };

    yode_core::plugins::PluginRegistry::discover(&project_root)
        .enabled_workflow_paths()
        .into_iter()
        .flat_map(expand_workflow_contribution)
        .collect()
}

fn expand_workflow_contribution(path: PathBuf) -> Vec<PathBuf> {
    if path.is_dir() {
        return std::fs::read_dir(path)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect();
    }

    if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        vec![path]
    } else {
        Vec::new()
    }
}

fn workflow_project_root(dir: &Path) -> Option<PathBuf> {
    if dir.file_name().and_then(|name| name.to_str()) != Some("workflows") {
        return None;
    }
    let parent = dir.parent()?;
    if parent.file_name().and_then(|name| name.to_str()) == Some(".yode")
        || parent.file_name().and_then(|name| name.to_str()) == Some(".claude")
    {
        parent.parent().map(Path::to_path_buf)
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    use super::{load_workflow_definition, workflow_definition_paths};

    #[test]
    fn enabled_plugin_workflows_are_discovered() {
        let root =
            std::env::temp_dir().join(format!("yode-plugin-workflows-{}", uuid::Uuid::new_v4()));
        let workflows_dir = root.join(".yode").join("workflows");
        let plugin_dir = root.join(".yode").join("plugins").join("demo");
        let plugin_workflows = plugin_dir.join("workflows");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&workflows_dir).unwrap();
        std::fs::create_dir_all(&plugin_workflows).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            r#"
name = "demo"
trust = "enabled"
workflows = ["workflows/plugin-review.json"]
"#,
        )
        .unwrap();
        std::fs::write(
            plugin_workflows.join("plugin-review.json"),
            r#"{"description":"plugin workflow","steps":[{"tool_name":"review_changes"}]}"#,
        )
        .unwrap();

        let paths = workflow_definition_paths(&workflows_dir);
        let (path, _json, steps) =
            load_workflow_definition(&workflows_dir, "plugin-review").unwrap();

        assert!(paths
            .iter()
            .any(|path| path.ends_with("plugin-review.json")));
        assert!(path.ends_with("plugin-review.json"));
        assert_eq!(steps.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn disabled_plugin_workflows_are_not_discovered() {
        let root =
            std::env::temp_dir().join(format!("yode-plugin-workflows-{}", uuid::Uuid::new_v4()));
        let workflows_dir = root.join(".yode").join("workflows");
        let plugin_dir = root.join(".yode").join("plugins").join("demo");
        let plugin_workflows = plugin_dir.join("workflows");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&workflows_dir).unwrap();
        std::fs::create_dir_all(&plugin_workflows).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            r#"
name = "demo"
trust = "disabled"
workflows = ["workflows/plugin-review.json"]
"#,
        )
        .unwrap();
        std::fs::write(
            plugin_workflows.join("plugin-review.json"),
            r#"{"description":"plugin workflow","steps":[{"tool_name":"review_changes"}]}"#,
        )
        .unwrap();

        let paths = workflow_definition_paths(&workflows_dir);

        assert!(!paths
            .iter()
            .any(|path| path.ends_with("plugin-review.json")));
        let _ = std::fs::remove_dir_all(&root);
    }
}
