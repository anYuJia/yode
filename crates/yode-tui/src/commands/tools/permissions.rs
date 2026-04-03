use crate::commands::{Command, CommandMeta, CommandCategory, CommandOutput, CommandResult, ArgDef, ArgCompletionSource};
use crate::commands::context::CommandContext;

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
                            names.extend(
                                ctx.tools.definitions()
                                    .iter()
                                    .map(|d| d.name.clone())
                            );
                            names.sort();
                            names
                        }),
                    },
                    ArgDef {
                        name: "action".into(),
                        required: false,
                        hint: "<allow|deny|default|plan|auto|accept-edits|bypass>".into(),
                        completions: ArgCompletionSource::Static(vec![
                            "allow".into(), "deny".into(),
                            "default".into(), "plan".into(), "auto".into(),
                            "accept-edits".into(), "bypass".into(),
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
    fn meta(&self) -> &CommandMeta { &self.meta }

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
                let mut lines = vec![
                    format!("Permission mode: {}", mode),
                ];
                if tools.is_empty() {
                    lines.push("All tools are auto-allowed (no confirmations required).".into());
                } else {
                    lines.push("Tools requiring confirmation:".into());
                    for t in tools {
                        lines.push(format!("  {t}"));
                    }
                }
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
            ["mode", mode_str] => {
                match mode_str.parse::<yode_core::PermissionMode>() {
                    Ok(mode) => {
                        engine.permissions_mut().set_mode(mode);
                        Ok(CommandOutput::Message(format!("Permission mode set to: {}", mode)))
                    }
                    Err(e) => Err(e),
                }
            }
            // Reset
            ["reset"] => {
                engine.permissions_mut().reset(vec![
                    "bash".into(),
                    "write_file".into(),
                    "edit_file".into(),
                ]);
                Ok(CommandOutput::Message("Permissions reset to defaults.".into()))
            }
            // /permissions <tool> allow
            [tool, "allow"] => {
                engine.permissions_mut().allow(tool);
                Ok(CommandOutput::Message(format!("Tool '{tool}' set to auto-allow.")))
            }
            // /permissions <tool> deny
            [tool, "deny"] => {
                engine.permissions_mut().deny(tool);
                Ok(CommandOutput::Message(format!("Tool '{tool}' now requires confirmation.")))
            }
            _ => Err("Usage: /permissions [mode <mode>] | [tool allow|deny] | [reset]".into()),
        }
    }
}
