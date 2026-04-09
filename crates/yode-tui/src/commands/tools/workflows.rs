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
                        hint: "[run <name>]".into(),
                        completions: ArgCompletionSource::Static(vec!["run".to_string()]),
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
            ctx.input.set_text(&format!(
                "Use `workflow_run` with name=\"{}\" and summarize the result.",
                name
            ));
            return Ok(CommandOutput::Message(format!(
                "Loaded a workflow_run prompt for '{}'.",
                name
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
                            path.file_stem().and_then(|value| value.to_str()).unwrap_or("workflow")
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
            "\nUse `/workflows run <name>` to load a workflow_run prompt, or call the `workflow_run` tool directly.",
        );
        Ok(CommandOutput::Message(output))
    }
}
