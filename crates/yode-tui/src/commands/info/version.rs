use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct VersionCommand {
    meta: CommandMeta,
}

impl VersionCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "version",
                description: "Show version information",
                aliases: &["v"],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for VersionCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
        Ok(CommandOutput::Message(format!(
            "yode {}\n  Built with:  rustc ({})\n  OS:          {} {}\n  Profile:     {}",
            env!("CARGO_PKG_VERSION"),
            option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("unknown"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        )))
    }
}
