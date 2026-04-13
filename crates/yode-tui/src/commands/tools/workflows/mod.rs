mod actions;
mod definitions;
#[cfg(test)]
mod tests;
mod workspace;

use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandResult,
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
        actions::execute_workflows_command(args, ctx, &dir)
    }
}
