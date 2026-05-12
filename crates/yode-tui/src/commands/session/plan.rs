use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

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
                args: vec![ArgDef {
                    name: "mode".to_string(),
                    required: false,
                    hint: "on | off | status".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "on".to_string(),
                        "off".to_string(),
                        "status".to_string(),
                    ]),
                }],
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

        match plan_action(trimmed) {
            PlanAction::Enable => {
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
            PlanAction::Disable => {
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
            PlanAction::Status => Ok(CommandOutput::Message(format!(
                "Plan mode: {}. Permission mode: {}.",
                if engine.permissions().mode() == yode_core::PermissionMode::Plan {
                    "enabled"
                } else {
                    "disabled"
                },
                engine.permissions().mode()
            ))),
            PlanAction::Invalid => Err("Usage: /plan [on|off|status]".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanAction {
    Enable,
    Disable,
    Status,
    Invalid,
}

fn plan_action(value: &str) -> PlanAction {
    match value {
        "" | "on" | "enter" => PlanAction::Enable,
        "off" | "exit" | "default" => PlanAction::Disable,
        "status" | "show" | "current" => PlanAction::Status,
        _ => PlanAction::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::{plan_action, PlanAction};

    #[test]
    fn plan_action_accepts_status_aliases() {
        assert_eq!(plan_action(""), PlanAction::Enable);
        assert_eq!(plan_action("on"), PlanAction::Enable);
        assert_eq!(plan_action("off"), PlanAction::Disable);
        assert_eq!(plan_action("status"), PlanAction::Status);
        assert_eq!(plan_action("current"), PlanAction::Status);
        assert_eq!(plan_action("bad"), PlanAction::Invalid);
    }
}
