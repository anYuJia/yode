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
                description: "View or modify tool execution permissions",
                aliases: &["perms"],
                args: vec![
                    ArgDef {
                        name: "tool".into(),
                        required: false,
                        hint: "<tool-name|reset>".into(),
                        completions: ArgCompletionSource::Dynamic(|ctx| {
                            let mut names: Vec<String> = ctx.tools.definitions()
                                .iter()
                                .map(|d| d.name.clone())
                                .collect();
                            names.push("reset".into());
                            names.sort();
                            names
                        }),
                    },
                    ArgDef {
                        name: "action".into(),
                        required: false,
                        hint: "<allow|deny>".into(),
                        completions: ArgCompletionSource::Static(vec!["allow".into(), "deny".into()]),
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
            // No args: show current permissions
            [] => {
                let tools = engine.permissions().confirmable_tools();
                if tools.is_empty() {
                    Ok(CommandOutput::Message("All tools are auto-allowed (no confirmations required).".into()))
                } else {
                    let mut lines = vec!["Tools requiring confirmation:".to_string()];
                    for t in tools {
                        lines.push(format!("  {t}"));
                    }
                    Ok(CommandOutput::Messages(lines))
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
            _ => Err("Usage: /permissions [tool] [allow|deny] or /permissions reset".into()),
        }
    }
}
