use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use yode_core::cost_tracker::estimate_token_cost;

pub struct CostCommand {
    meta: CommandMeta,
}

impl CostCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "cost",
                description: "Show token usage and estimated cost",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for CostCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let cost = estimate_token_cost(
            &ctx.session.model,
            ctx.session.input_tokens.into(),
            ctx.session.output_tokens.into(),
        );
        Ok(CommandOutput::Message(format!(
            "Token usage:\n  Input tokens:  {}\n  Output tokens: {}\n  Total tokens:  {}\n  Tool calls:    {}\n  Est. cost:     ${:.4}",
            ctx.session.input_tokens,
            ctx.session.output_tokens,
            ctx.session.total_tokens,
            ctx.session.tool_call_count,
            cost,
        )))
    }
}
