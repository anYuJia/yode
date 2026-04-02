use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct DoctorCommand {
    meta: CommandMeta,
}

impl DoctorCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "doctor",
                description: "Run environment health check",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for DoctorCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let mut checks = Vec::new();

        // Check API key
        let has_api_key =
            std::env::var("ANTHROPIC_API_KEY").is_ok() || std::env::var("OPENAI_API_KEY").is_ok();
        checks.push(if has_api_key {
            "  [ok] API key configured"
        } else {
            "  [!!] No API key found (ANTHROPIC_API_KEY or OPENAI_API_KEY)"
        });

        // Check git
        let git_ok = std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        checks.push(if git_ok {
            "  [ok] git available"
        } else {
            "  [!!] git not found"
        });

        // Check terminal capabilities
        checks.push(if ctx.terminal_caps.truecolor {
            "  [ok] Truecolor support"
        } else {
            "  [--] No truecolor (using 256 colors)"
        });
        if ctx.terminal_caps.in_tmux {
            checks.push("  [--] Running inside tmux");
        }
        if ctx.terminal_caps.in_ssh {
            checks.push("  [--] Running over SSH");
        }

        // Check tools
        let tool_count = ctx.tools.definitions().len();
        checks.push(if tool_count > 0 {
            "  [ok] Tools registered"
        } else {
            "  [!!] No tools registered"
        });

        Ok(CommandOutput::Message(format!(
            "Environment check:\n{}\n\n  Terminal: {}\n  Tools:    {} registered",
            checks.join("\n"),
            ctx.terminal_caps.summary(),
            tool_count,
        )))
    }
}
