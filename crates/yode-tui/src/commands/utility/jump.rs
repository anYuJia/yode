use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct JumpCommand {
    meta: CommandMeta,
}

impl JumpCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "jump",
                description: "Load a workspace jump command into the input box",
                aliases: &[],
                args: vec![ArgDef {
                    name: "target".to_string(),
                    required: false,
                    hint: "[task|memory|review|status|diagnostics|doctor]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "task".to_string(),
                        "memory".to_string(),
                        "review".to_string(),
                        "status".to_string(),
                        "diagnostics".to_string(),
                        "doctor".to_string(),
                    ]),
                }],
                category: CommandCategory::Utility,
                hidden: false,
            },
        }
    }
}

impl Command for JumpCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let command = match args.trim() {
            "" | "task" => "/tasks latest",
            "memory" => "/memory latest",
            "review" => "/reviews latest",
            "status" => "/status",
            "diagnostics" => "/diagnostics",
            "doctor" => "/doctor bundle",
            other => return Err(format!("Unknown jump target '{}'.", other)),
        };
        ctx.input.set_text(command);
        Ok(CommandOutput::Message(format!(
            "Loaded jump target `{}` into the input box.",
            command
        )))
    }
}
