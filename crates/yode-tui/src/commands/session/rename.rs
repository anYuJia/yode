use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct RenameCommand {
    meta: CommandMeta,
}

impl RenameCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "rename",
                description: "Rename the current session",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for RenameCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let new_name = args.trim();

        if new_name.is_empty() {
            return Ok(CommandOutput::Message("Usage: /rename <name>".to_string()));
        }

        // Update session name in memory
        let session_id = ctx.session.session_id.clone();

        // Update in database if available
        if let Ok(engine) = ctx.engine.try_lock() {
            if let Some(db) = engine.get_database() {
                if let Err(e) = db.update_session_name(&session_id, new_name) {
                    return Ok(CommandOutput::Message(format!(
                        "Failed to update session name in database: {}",
                        e
                    )));
                }
            }
        }

        Ok(CommandOutput::Message(format!(
            "Session renamed to: {}",
            new_name
        )))
    }
}
