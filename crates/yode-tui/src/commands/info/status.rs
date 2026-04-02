use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

use super::cost::estimate_cost;

pub struct StatusCommand {
    meta: CommandMeta,
}

impl StatusCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "status",
                description: "Show session status",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for StatusCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let session_short = &ctx.session.session_id[..ctx.session.session_id.len().min(8)];
        let always_allow = if ctx.session.always_allow_tools.is_empty() {
            "none".to_string()
        } else {
            ctx.session.always_allow_tools.join(", ")
        };
        let cost = estimate_cost(
            &ctx.session.model,
            ctx.session.input_tokens,
            ctx.session.output_tokens,
        );

        Ok(CommandOutput::Message(format!(
            "Session status:\n  Session:         {}\n  Model:           {}\n  Working dir:     {}\n  Permission mode: {}\n  Tokens:          {} (in: {}, out: {})\n  Tool calls:      {}\n  Est. cost:       ${:.4}\n  Always-allow:    {}\n  Terminal:        {}",
            session_short,
            ctx.session.model,
            ctx.session.working_dir,
            ctx.session.permission_mode.label(),
            ctx.session.total_tokens,
            ctx.session.input_tokens,
            ctx.session.output_tokens,
            ctx.session.tool_call_count,
            cost,
            always_allow,
            ctx.terminal_caps.summary(),
        )))
    }
}
