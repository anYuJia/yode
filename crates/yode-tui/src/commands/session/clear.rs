use crate::commands::context::CommandContext;
use crate::commands::{ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct ClearCommand {
    meta: CommandMeta,
}

impl ClearCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "clear",
                description: "Clear conversation history and start fresh (preserves token stats)",
                aliases: &["cls", "reset"],
                args: vec![ArgDef {
                    name: "options".to_string(),
                    required: false,
                    hint: "[--stats]".to_string(),
                    completions: ArgCompletionSource::Static(vec!["--stats".to_string()]),
                }],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for ClearCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        // Parse optional arguments
        let reset_stats = args.contains("--stats");

        // Clear chat entries
        ctx.chat_entries.clear();

        // Reset turn tokens
        ctx.session.turn_input_tokens = 0;
        ctx.session.turn_output_tokens = 0;

        // Reset engine conversation
        if let Ok(mut engine) = ctx.engine.try_lock() {
            engine.clear_conversation();
        }

        // Optionally reset total stats
        if reset_stats {
            ctx.session.input_tokens = 0;
            ctx.session.output_tokens = 0;
            ctx.session.total_tokens = 0;
            ctx.session.tool_call_count = 0;
        }

        // Generate new session ID for clean break
        ctx.session.session_id = uuid::Uuid::new_v4().to_string();

        let mut message = String::from("Conversation cleared. Starting fresh session.");
        if reset_stats {
            message.push_str(" Token stats reset.");
        }

        Ok(CommandOutput::Message(message))
    }
}
