use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct PermissionsCommand {
    meta: CommandMeta,
}

impl PermissionsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "permissions",
                description: "View or modify tool execution permissions and permission mode",
                aliases: &["perms"],
                args: vec![
                    ArgDef {
                        name: "subcommand".into(),
                        required: false,
                        hint: "<mode|tool-name|reset>".into(),
                        completions: ArgCompletionSource::Dynamic(|ctx| {
                            let mut names: Vec<String> = vec!["mode".into(), "reset".into()];
                            names.extend(ctx.tools.definitions().iter().map(|d| d.name.clone()));
                            names.sort();
                            names
                        }),
                    },
                    ArgDef {
                        name: "action".into(),
                        required: false,
                        hint: "<allow|deny|default|plan|auto|accept-edits|bypass>".into(),
                        completions: ArgCompletionSource::Static(vec![
                            "allow".into(),
                            "deny".into(),
                            "default".into(),
                            "plan".into(),
                            "auto".into(),
                            "accept-edits".into(),
                            "bypass".into(),
                        ]),
                    },
                ],
                category: CommandCategory::Tools,
                hidden: false,
            },
        }
    }
}
impl Command for PermissionsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let parts: Vec<&str> = args.trim().split_whitespace().collect();

        let Ok(mut engine) = ctx.engine.try_lock() else {
            return Err("Engine is busy, try again.".into());
        };

        match parts.as_slice() {
            // No args: show current permissions and mode
            [] => {
                let mode = engine.permissions().mode();
                let tools = engine.permissions().confirmable_tools();
                let rules = engine.permissions().rules_snapshot();
                let denials = engine.permissions().recent_denials(5);
                let runtime = engine.runtime_state();
                let mut lines = vec![
                    format!("Permission mode: {}", mode),
                    format!("Recovery state: {}", runtime.recovery_state),
                ];
                if tools.is_empty() {
                    lines.push("All tools are auto-allowed (no confirmations required).".into());
                } else {
                    lines.push("Tools requiring confirmation:".into());
                    for t in tools {
                        lines.push(format!("  {t}"));
                    }
                }
                if rules.is_empty() {
                    lines.push("Rules: none".into());
                } else {
                    lines.push("Rules:".into());
                    for rule in rules {
                        lines.push(format!(
                            "  {:?} {} {}{}",
                            rule.source,
                            rule.tool_name,
                            match rule.behavior {
                                yode_core::permission::RuleBehavior::Allow => "allow",
                                yode_core::permission::RuleBehavior::Deny => "deny",
                                yode_core::permission::RuleBehavior::Ask => "ask",
                            },
                            rule.pattern
                                .as_ref()
                                .map(|pattern| format!(" ({})", pattern))
                                .unwrap_or_default()
                        ));
                    }
                }
                if denials.is_empty() {
                    lines.push("Recent denials: none".into());
                } else {
                    lines.push("Recent denials:".into());
                    for denial in denials {
                        lines.push(format!(
                            "  {} x{} (consecutive {}, at {})",
                            denial.tool_name, denial.count, denial.consecutive, denial.last_at
                        ));
                    }
                }
                lines.push(format!(
                    "Last permission decision: {} [{}]",
                    runtime.last_permission_tool.as_deref().unwrap_or("none"),
                    runtime.last_permission_action.as_deref().unwrap_or("none")
                ));
                lines.push(format!(
                    "Why: {}",
                    runtime
                        .last_permission_explanation
                        .as_deref()
                        .unwrap_or("none")
                ));
                Ok(CommandOutput::Messages(lines))
            }
            // /permissions mode — show current mode
            ["mode"] => {
                let mode = engine.permissions().mode();
                Ok(CommandOutput::Message(format!(
                    "Current permission mode: {}\n\
                     Available modes: default, plan, auto, accept-edits, bypass\n\
                     Usage: /permissions mode <mode-name>",
                    mode
                )))
            }
            // /permissions mode <mode-name>
            ["mode", mode_str] => match mode_str.parse::<yode_core::PermissionMode>() {
                Ok(mode) => {
                    engine.permissions_mut().set_mode(mode);
                    Ok(CommandOutput::Message(format!(
                        "Permission mode set to: {}",
                        mode
                    )))
                }
                Err(e) => Err(e),
            },
            // Reset
            ["reset"] => {
                engine.permissions_mut().reset(vec![
                    "bash".into(),
                    "write_file".into(),
                    "edit_file".into(),
                ]);
                Ok(CommandOutput::Message(
                    "Permissions reset to defaults.".into(),
                ))
            }
            // /permissions <tool> allow
            [tool, "allow"] => {
                engine.permissions_mut().allow(tool);
                Ok(CommandOutput::Message(format!(
                    "Tool '{tool}' set to auto-allow."
                )))
            }
            // /permissions <tool> deny
            [tool, "deny"] => {
                engine.permissions_mut().deny(tool);
                Ok(CommandOutput::Message(format!(
                    "Tool '{tool}' set to deny."
                )))
            }
            _ => Err("Usage: /permissions [mode <mode>] | [tool allow|deny] | [reset]".into()),
        }
    }
}
