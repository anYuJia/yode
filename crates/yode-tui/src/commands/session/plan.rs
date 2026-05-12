use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct PlanCommand {
    meta: CommandMeta,
}

impl PlanCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "plan",
                description: "Enter read-only planning mode",
                aliases: &[],
                args: Vec::new(),
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for PlanCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let trimmed = args.trim();
        let Ok(mut engine) = ctx.engine.try_lock() else {
            return Err("Engine is busy, try again.".into());
        };

        match trimmed {
            "" | "on" | "enter" => {
                engine
                    .permissions_mut()
                    .set_mode(yode_core::PermissionMode::Plan);
                if !engine.set_runtime_plan_mode(true) {
                    return Err("Engine plan-mode state is busy, try again.".to_string());
                }
                Ok(CommandOutput::Message(
                    "Plan mode enabled. Mutation tools are blocked until you switch permission mode."
                        .to_string(),
                ))
            }
            "off" | "exit" | "default" => {
                engine
                    .permissions_mut()
                    .set_mode(yode_core::PermissionMode::Default);
                if !engine.set_runtime_plan_mode(false) {
                    return Err("Engine plan-mode state is busy, try again.".to_string());
                }
                Ok(CommandOutput::Message(
                    "Plan mode disabled. Permission mode restored to default.".to_string(),
                ))
            }
            _ => Err("Usage: /plan [on|off]".to_string()),
        }
    }
}
