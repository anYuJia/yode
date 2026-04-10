use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct WorkflowsCommand {
    meta: CommandMeta,
}

impl WorkflowsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "workflows",
                description: "List workflow scripts or load a workflow_run prompt",
                aliases: &[],
                args: vec![
                    ArgDef {
                        name: "action".into(),
                        required: false,
                        hint: "[run|run-write|show|preview|init <name>]".into(),
                        completions: ArgCompletionSource::Static(vec![
                            "run".to_string(),
                            "run-write".to_string(),
                            "show".to_string(),
                            "preview".to_string(),
                            "init".to_string(),
                        ]),
                    },
                    ArgDef {
                        name: "name".into(),
                        required: false,
                        hint: "[workflow-name]".into(),
                        completions: ArgCompletionSource::None,
                    },
                ],
                category: CommandCategory::Tools,
                hidden: false,
            },
        }
    }
}

impl Command for WorkflowsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let dir = std::path::PathBuf::from(&ctx.session.working_dir)
            .join(".yode")
            .join("workflows");
        let parts = args.split_whitespace().collect::<Vec<_>>();
        if let ["run", name] = parts.as_slice() {
            let name = if *name == "latest" {
                latest_workflow_name(&dir)
                    .ok_or_else(|| "No workflow scripts found.".to_string())?
            } else {
                (*name).to_string()
            };
            ctx.input.set_text(&format!(
                "Use `workflow_run` with name=\"{}\" and summarize the result.",
                name
            ));
            return Ok(CommandOutput::Message(format!(
                "Loaded a workflow_run prompt for '{}'.",
                name
            )));
        }
        if let ["run-write", name] = parts.as_slice() {
            let name = if *name == "latest" {
                latest_workflow_name(&dir)
                    .ok_or_else(|| "No workflow scripts found.".to_string())?
            } else {
                (*name).to_string()
            };
            ctx.input.set_text(&format!(
                "Use `workflow_run_with_writes` with name=\"{}\". Explain why this workflow needs mutating tools, then summarize every file or git-side effect clearly.",
                name
            ));
            return Ok(CommandOutput::Message(format!(
                "Loaded a write-enabled workflow prompt for '{}'.",
                name
            )));
        }
        if let ["show", name] = parts.as_slice() {
            let name = if *name == "latest" {
                latest_workflow_name(&dir)
                    .ok_or_else(|| "No workflow scripts found.".to_string())?
            } else {
                (*name).to_string()
            };
            let (path, json, steps) = load_workflow_definition(&dir, &name)?;
            let write_capable = workflow_requires_write_mode(&steps);
            let write_steps = steps
                .iter()
                .enumerate()
                .filter_map(|(index, step)| {
                    let tool_name = step.get("tool_name").and_then(|value| value.as_str())?;
                    (!is_safe_workflow_step(tool_name))
                        .then(|| format!("{}. {}", index + 1, tool_name))
                })
                .collect::<Vec<_>>();
            let mut output = format!(
                "Workflow {}\nPath: {}\nDescription: {}\nMode: {}\n\nSteps:\n",
                json.get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or(name.as_str()),
                path.display(),
                json.get("description")
                    .and_then(|value| value.as_str())
                    .unwrap_or("none"),
                if write_capable {
                    "write-capable (use /workflows run-write)"
                } else {
                    "safe read-only (use /workflows run)"
                },
            );
            for (index, step) in steps.iter().enumerate() {
                let tool_name = step
                    .get("tool_name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown");
                output.push_str(&format!(
                    "  {}. {} [{}] {}\n",
                    index + 1,
                    tool_name,
                    if is_safe_workflow_step(tool_name) {
                        "safe"
                    } else {
                        "write"
                    },
                    if step
                        .get("continue_on_error")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false)
                    {
                        "(continue_on_error)"
                    } else {
                        ""
                    }
                ));
            }
            if !write_steps.is_empty() {
                output.push_str("\nWrite-capable steps:\n");
                for step in write_steps {
                    output.push_str(&format!("  - {}\n", step));
                }
            }
            output.push_str(
                "\nUse `/workflows run <name>` for safe workflows, `/workflows run-write <name>` for confirmed write-capable workflows, or call `workflow_run` with dry_run=true.",
            );
            return Ok(CommandOutput::Message(output));
        }
        if let ["preview", name] = parts.as_slice() {
            let name = if *name == "latest" {
                latest_workflow_name(&dir)
                    .ok_or_else(|| "No workflow scripts found.".to_string())?
            } else {
                (*name).to_string()
            };
            let (path, json, steps) = load_workflow_definition(&dir, &name)?;
            let mut output = format!(
                "Workflow plan preview\nName: {}\nPath: {}\nMode: {}\nDescription: {}\n\nPlan:\n",
                json.get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or(name.as_str()),
                path.display(),
                if workflow_requires_write_mode(&steps) {
                    "write-capable"
                } else {
                    "safe read-only"
                },
                json.get("description")
                    .and_then(|value| value.as_str())
                    .unwrap_or("none"),
            );
            for (index, step) in steps.iter().enumerate() {
                let tool_name = step
                    .get("tool_name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown");
                let params_preview = step
                    .get("params")
                    .map(compact_json_preview)
                    .unwrap_or_else(|| "{}".to_string());
                output.push_str(&format!(
                    "  {}. {} [{}]{}\n     params: {}\n",
                    index + 1,
                    tool_name,
                    if is_safe_workflow_step(tool_name) {
                        "safe"
                    } else {
                        "write"
                    },
                    if step
                        .get("continue_on_error")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false)
                    {
                        " continue_on_error"
                    } else {
                        ""
                    },
                    params_preview
                ));
            }
            output.push_str(
                "\nUse `/workflows show <name>` for the raw definition, `/workflows run <name>` for safe execution, or `/workflows run-write <name>` after confirming mutating steps.",
            );
            return Ok(CommandOutput::Message(output));
        }
        if let ["init", template] = parts.as_slice() {
            std::fs::create_dir_all(&dir)
                .map_err(|err| format!("Failed to create {}: {}", dir.display(), err))?;
            let (file_name, content) = workflow_template(template).ok_or_else(|| {
                format!(
                    "Unknown workflow template: {}. Available: {}",
                    template,
                    workflow_template_names().join(", ")
                )
            })?;
            let path = dir.join(file_name);
            std::fs::write(&path, content)
                .map_err(|err| format!("Failed to write {}: {}", path.display(), err))?;
            return Ok(CommandOutput::Message(format!(
                "Initialized workflow template '{}' at {}.",
                template,
                path.display()
            )));
        }

        let entries = std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect::<Vec<_>>();

        if entries.is_empty() {
            return Ok(CommandOutput::Message(format!(
                "No workflow scripts found in {}.",
                dir.display()
            )));
        }

        let mut output = format!("Workflow scripts in {}:\n", dir.display());
        for path in entries {
            let label = std::fs::read_to_string(&path)
                .ok()
                .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
                .map(|json| {
                    let mode = json
                        .get("steps")
                        .and_then(|value| value.as_array())
                        .map(|steps| {
                            if workflow_requires_write_mode(steps) {
                                "write"
                            } else {
                                "safe"
                            }
                        })
                        .unwrap_or("unknown");
                    let name = json
                        .get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or_else(|| {
                            path.file_stem()
                                .and_then(|value| value.to_str())
                                .unwrap_or("workflow")
                        });
                    let description = json
                        .get("description")
                        .and_then(|value| value.as_str())
                        .unwrap_or("no description");
                    format!("{} [{}] — {}", name, mode, description)
                })
                .unwrap_or_else(|| path.display().to_string());
            output.push_str(&format!("  - {} ({})\n", label, path.display()));
        }
        output.push_str(
            "\nUse `/workflows run <name>` for safe workflows or `/workflows run-write <name>` for confirmed write-capable workflows.",
        );
        Ok(CommandOutput::Message(output))
    }
}

fn latest_workflow_name(dir: &std::path::Path) -> Option<String> {
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

fn load_workflow_definition(
    dir: &std::path::Path,
    name: &str,
) -> Result<(std::path::PathBuf, serde_json::Value, Vec<serde_json::Value>), String> {
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

fn compact_json_preview(value: &serde_json::Value) -> String {
    let raw = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    if raw.chars().count() > 120 {
        format!("{}...", raw.chars().take(120).collect::<String>())
    } else {
        raw
    }
}

fn workflow_template_names() -> Vec<&'static str> {
    vec![
        "review-pipeline",
        "review-then-commit",
        "ship-pipeline",
        "coordinator-review",
    ]
}

fn workflow_template(name: &str) -> Option<(&'static str, &'static str)> {
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

fn workflow_requires_write_mode(steps: &[serde_json::Value]) -> bool {
    steps.iter().any(|step| {
        step.get("tool_name")
            .and_then(|value| value.as_str())
            .map(|tool_name| !is_safe_workflow_step(tool_name))
            .unwrap_or(true)
    })
}

fn is_safe_workflow_step(tool_name: &str) -> bool {
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
    use super::{compact_json_preview, workflow_requires_write_mode, workflow_template};

    #[test]
    fn workflow_mode_detection_distinguishes_safe_and_write_steps() {
        let safe = serde_json::json!([
            { "tool_name": "review_changes" },
            { "tool_name": "verification_agent" }
        ]);
        let write = serde_json::json!([
            { "tool_name": "review_pipeline" }
        ]);

        assert!(!workflow_requires_write_mode(safe.as_array().unwrap()));
        assert!(workflow_requires_write_mode(write.as_array().unwrap()));
    }

    #[test]
    fn workflow_templates_include_ship_flows() {
        assert!(workflow_template("review-then-commit").is_some());
        assert!(workflow_template("ship-pipeline").is_some());
    }

    #[test]
    fn compact_json_preview_truncates_long_params() {
        let preview = compact_json_preview(&serde_json::json!({
            "focus": "x".repeat(200)
        }));
        assert!(preview.ends_with("..."));
    }
}
