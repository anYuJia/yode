use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use crate::commands::info::permission_recovery_workspace::{
    render_permission_workspace, render_recovery_workspace,
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
                            let mut names: Vec<String> = vec![
                                "mode".into(),
                                "reset".into(),
                                "explain".into(),
                                "denials".into(),
                            ];
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
                let denial_prefixes = engine.permissions().recent_denial_prefixes(5);
                let safe_prefixes = engine.permissions().safe_readonly_shell_prefixes();
                let confirmation_suggestions = engine.permissions().confirmation_rule_suggestions(3);
                let runtime = engine.runtime_state();
                let denial_prefix_lines = denial_prefixes
                    .iter()
                    .map(|denial| {
                        format!(
                            "{} -> count={} consecutive={} last_at={}",
                            denial.prefix, denial.count, denial.consecutive, denial.last_at
                        )
                    })
                    .collect::<Vec<_>>();
                let denial_lines = if denials.is_empty() {
                    vec!["none".to_string()]
                } else {
                    denials
                        .into_iter()
                        .map(|denial| {
                            format!(
                                "{} x{} (consecutive {}, at {})",
                                denial.tool_name, denial.count, denial.consecutive, denial.last_at
                            )
                        })
                        .collect()
                };
                Ok(CommandOutput::Message(format!(
                    "{}\n\n{}",
                    render_permission_workspace(
                        mode,
                        &tools,
                        &rules,
                        &denial_lines,
                        &denial_prefix_lines,
                        &safe_prefixes.join(", "),
                        &confirmation_suggestions,
                        &runtime,
                    ),
                    render_recovery_workspace(&runtime),
                )))
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
            // /permissions denials [tool]
            ["denials"] | ["denials", _] => {
                let filter = parts.get(1).copied();
                let denials = engine.permissions().recent_denials(20);
                let filtered = denials
                    .into_iter()
                    .filter(|denial| {
                        filter
                            .map(|tool| denial.tool_name == tool)
                            .unwrap_or(true)
                    })
                    .collect::<Vec<_>>();
                if filtered.is_empty() {
                    let prefix_lines = engine.permissions().recent_denial_prefixes(10);
                    if prefix_lines.is_empty() {
                        return Ok(CommandOutput::Message(
                            "Recent denials: none".to_string(),
                        ));
                    }
                    let mut lines =
                        vec!["Recent bash denials grouped by command prefix:".to_string()];
                    for denial in prefix_lines {
                        lines.push(format!(
                            "  {} -> count={} consecutive={} last_at={}",
                            denial.prefix, denial.count, denial.consecutive, denial.last_at
                        ));
                    }
                    return Ok(CommandOutput::Messages(lines));
                }
                let mut lines = vec!["Recent denials grouped by tool:".to_string()];
                for denial in filtered {
                    lines.push(format!(
                        "  {} -> count={} consecutive={} last_at={}",
                        denial.tool_name, denial.count, denial.consecutive, denial.last_at
                    ));
                }
                Ok(CommandOutput::Messages(lines))
            }
            // /permissions explain <tool> [content]
            ["explain", tool, content @ ..] => {
                let content = (!content.is_empty()).then(|| content.join(" "));
                let explanation = engine
                    .permissions()
                    .explain_with_content(tool, content.as_deref());
                Ok(CommandOutput::Message(format!(
                    "Permission explanation for '{}':\n  Action:      {}\n  Mode:        {}\n  Reason:      {}\n  Matched rule: {}\n  Risk:        {}\n  Denials:     {}{}\n",
                    tool,
                    explanation.action.label(),
                    explanation.mode,
                    explanation.reason,
                    explanation.matched_rule.as_deref().unwrap_or("none"),
                    explanation
                        .classifier_risk
                        .map(|risk| format!("{:?}", risk))
                        .unwrap_or_else(|| "none".to_string()),
                    explanation.denial_count,
                    if explanation.auto_skip_due_to_denials {
                        " (auto-skip active)"
                    } else {
                        ""
                    }
                )))
            }
            // /permissions <tool> explain [content]
            [tool, "explain", content @ ..] => {
                let content = (!content.is_empty()).then(|| content.join(" "));
                let explanation = engine
                    .permissions()
                    .explain_with_content(tool, content.as_deref());
                Ok(CommandOutput::Message(format!(
                    "Permission explanation for '{}':\n  Action:      {}\n  Mode:        {}\n  Reason:      {}\n  Matched rule: {}\n  Risk:        {}\n  Denials:     {}{}\n",
                    tool,
                    explanation.action.label(),
                    explanation.mode,
                    explanation.reason,
                    explanation.matched_rule.as_deref().unwrap_or("none"),
                    explanation
                        .classifier_risk
                        .map(|risk| format!("{:?}", risk))
                        .unwrap_or_else(|| "none".to_string()),
                    explanation.denial_count,
                    if explanation.auto_skip_due_to_denials {
                        " (auto-skip active)"
                    } else {
                        ""
                    }
                )))
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
            _ => Err("Usage: /permissions [mode <mode>] | [denials [tool]] | [explain <tool> [content]] | [tool allow|deny|explain] | [reset]".into()),
        }
    }
}
