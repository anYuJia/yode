use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct DiffCommand {
    meta: CommandMeta,
}

impl DiffCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "diff",
                description: "Show git diff summary",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Development,
                hidden: false,
            },
        }
    }
}

impl Command for DiffCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
        let output = std::process::Command::new("git")
            .args(["diff", "--stat"])
            .output();
        let content = match output {
            Ok(o) if o.status.success() => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                if stdout.is_empty() {
                    "No uncommitted changes.".to_string()
                } else {
                    stdout.to_string()
                }
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                format!("git error: {}", stderr.trim())
            }
            Err(e) => format!("Failed to run git: {}", e),
        };
        Ok(CommandOutput::Message(format!("Git diff:\n{}", content)))
    }
}
