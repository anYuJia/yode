use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

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
        let cost = estimate_cost(
            &ctx.session.model,
            ctx.session.input_tokens,
            ctx.session.output_tokens,
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

/// Estimate cost based on model with separate input/output pricing (per Mtok).
pub(crate) fn estimate_cost(model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
    let (input_per_mtok, output_per_mtok) =
        if model.contains("claude-3-opus") || model.contains("claude-opus") {
            (15.0, 75.0)
        } else if model.contains("claude-3-sonnet")
            || model.contains("claude-3.5")
            || model.contains("claude-sonnet")
        {
            (3.0, 15.0)
        } else if model.contains("claude-3-haiku") || model.contains("claude-haiku") {
            (0.25, 1.25)
        } else if model.contains("gpt-4o") {
            (2.5, 10.0)
        } else if model.contains("gpt-4") {
            (30.0, 60.0)
        } else if model.contains("gpt-3.5") {
            (0.5, 1.5)
        } else if model.contains("deepseek") {
            (0.14, 0.28)
        } else {
            (5.0, 15.0)
        };
    (input_tokens as f64 / 1_000_000.0) * input_per_mtok
        + (output_tokens as f64 / 1_000_000.0) * output_per_mtok
}
