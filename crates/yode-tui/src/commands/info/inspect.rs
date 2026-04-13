use crate::commands::context::CommandContext;
use crate::commands::inspector_bridge::document_from_command_output;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct InspectCommand {
    meta: CommandMeta,
}

impl InspectCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "inspect",
                description: "Open an inspector view for an existing command output",
                aliases: &[],
                args: vec![ArgDef {
                    name: "target".to_string(),
                    required: false,
                    hint: "[tasks|memory|reviews|status|diagnostics|doctor|hooks|permissions]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "tasks".to_string(),
                        "memory".to_string(),
                        "reviews".to_string(),
                        "status".to_string(),
                        "diagnostics".to_string(),
                        "doctor".to_string(),
                        "hooks".to_string(),
                        "permissions".to_string(),
                    ]),
                }],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for InspectCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let trimmed = args.trim();
        let (command, command_args, title) = match trimmed {
            "" => ("status", "", "Status inspector".to_string()),
            value if value.starts_with("tasks") => (
                "tasks",
                value.strip_prefix("tasks").unwrap_or("").trim(),
                "Task inspector".to_string(),
            ),
            value if value.starts_with("memory") => (
                "memory",
                value.strip_prefix("memory").unwrap_or("").trim(),
                "Memory inspector".to_string(),
            ),
            value if value.starts_with("reviews") => (
                "reviews",
                value.strip_prefix("reviews").unwrap_or("").trim(),
                "Review inspector".to_string(),
            ),
            value if value.starts_with("doctor") => (
                "doctor",
                value.strip_prefix("doctor").unwrap_or("").trim(),
                "Doctor inspector".to_string(),
            ),
            "status" => ("status", "", "Status inspector".to_string()),
            "diagnostics" => ("diagnostics", "", "Diagnostics inspector".to_string()),
            "hooks" => ("hooks", "", "Hook inspector".to_string()),
            value if value.starts_with("permissions") => (
                "permissions",
                value.strip_prefix("permissions").unwrap_or("").trim(),
                "Permission inspector".to_string(),
            ),
            other => return Err(format!("Unknown inspect target '{}'.", other)),
        };

        let output = ctx
            .cmd_registry
            .execute_command(command, command_args, ctx)
            .ok_or_else(|| format!("Command '{}' not found.", command))??;

        match output {
            CommandOutput::Message(body) => Ok(CommandOutput::OpenInspector(
                document_from_command_output(&title, body.lines().map(str::to_string).collect()),
            )),
            CommandOutput::Messages(lines) => {
                Ok(CommandOutput::OpenInspector(document_from_command_output(&title, lines)))
            }
            CommandOutput::OpenInspector(doc) => Ok(CommandOutput::OpenInspector(doc)),
            CommandOutput::Silent => Err("Inspect target produced no output.".to_string()),
            CommandOutput::StartWizard(_) | CommandOutput::ReloadProvider { .. } => {
                Err("Inspect target is not viewable as an inspector.".to_string())
            }
        }
    }
}
