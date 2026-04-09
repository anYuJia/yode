use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct WorkflowsCommand {
    meta: CommandMeta,
}

impl WorkflowsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "workflows",
                description: "List workflow scripts under .yode/workflows",
                aliases: &[],
                args: vec![],
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

    fn execute(&self, _args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let dir = std::path::PathBuf::from(&ctx.session.working_dir)
            .join(".yode")
            .join("workflows");
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
            output.push_str(&format!("  - {}\n", path.display()));
        }
        output.push_str("\nUse the `workflow_run` tool with `name` or `workflow_path` to execute one.");
        Ok(CommandOutput::Message(output))
    }
}
