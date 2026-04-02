use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct ConfigCommand {
    meta: CommandMeta,
}

impl ConfigCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "config",
                description: "Show configuration summary",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for ConfigCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let permission_mode = ctx.session.permission_mode.label();
        let always_allow = if ctx.session.always_allow_tools.is_empty() {
            "none".to_string()
        } else {
            ctx.session.always_allow_tools.join(", ")
        };
        Ok(CommandOutput::Message(format!(
            "Configuration:\n  Model:           {}\n  Permission mode: {}\n  Working dir:     {}\n  Always-allow:    {}\n  Terminal:        {}\n  Truecolor:       {}\n  Tmux:            {}\n  SSH:             {}",
            ctx.session.model,
            permission_mode,
            ctx.session.working_dir,
            always_allow,
            ctx.terminal_caps.term_program.as_deref().unwrap_or("unknown"),
            ctx.terminal_caps.truecolor,
            ctx.terminal_caps.in_tmux,
            ctx.terminal_caps.in_ssh,
        )))
    }
}
