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
                        hint: "[run|run-write|show|init <name>]".into(),
                        completions: ArgCompletionSource::Static(vec![
                            "run".to_string(),
                            "run-write".to_string(),
                            "show".to_string(),
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
            let path = dir.join(format!("{}.json", name));
            let content = std::fs::read_to_string(&path)
                .map_err(|err| format!("Failed to read {}: {}", path.display(), err))?;
            let json: serde_json::Value = serde_json::from_str(&content)
                .map_err(|err| format!("Invalid workflow JSON {}: {}", path.display(), err))?;
            let steps = json
                .get("steps")
                .and_then(|value| value.as_array())
                .ok_or_else(|| format!("Workflow {} has no steps array.", path.display()))?;
            let mut output = format!(
                "Workflow {}\nPath: {}\nDescription: {}\n\nSteps:\n",
                json.get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or(name.as_str()),
                path.display(),
                json.get("description")
                    .and_then(|value| value.as_str())
                    .unwrap_or("none"),
            );
            for (index, step) in steps.iter().enumerate() {
                output.push_str(&format!(
                    "  {}. {} {}\n",
                    index + 1,
                    step.get("tool_name")
                        .and_then(|value| value.as_str())
                        .unwrap_or("unknown"),
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
            output.push_str(
                "\nUse `/workflows run <name>` for safe workflows, `/workflows run-write <name>` for confirmed write-capable workflows, or call `workflow_run` with dry_run=true.",
            );
            return Ok(CommandOutput::Message(output));
        }
        if let ["init", template] = parts.as_slice() {
            std::fs::create_dir_all(&dir)
                .map_err(|err| format!("Failed to create {}: {}", dir.display(), err))?;
            let (file_name, content) = workflow_template(template)
                .ok_or_else(|| format!("Unknown workflow template: {}", template))?;
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
                    format!("{} — {}", name, description)
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
