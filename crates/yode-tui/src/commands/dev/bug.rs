use crate::app::ChatRole;
use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct BugCommand {
    meta: CommandMeta,
}

impl BugCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "bug",
                description: "Generate bug report template",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Development,
                hidden: false,
            },
        }
    }
}

impl Command for BugCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let session_short =
            &ctx.session.session_id[..ctx.session.session_id.len().min(8)];
        let os_info = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        let recent_msgs: Vec<String> = ctx
            .chat_entries
            .iter()
            .rev()
            .take(5)
            .map(|e| {
                let role = match &e.role {
                    ChatRole::User => "User",
                    ChatRole::Assistant => "Assistant",
                    ChatRole::System => "System",
                    ChatRole::ToolCall { name } => {
                        return format!("  ToolCall({}): ...", name)
                    }
                    ChatRole::ToolResult { name, .. } => {
                        return format!("  ToolResult({}): ...", name)
                    }
                    _ => "Other",
                };
                let preview: String = e.content.chars().take(80).collect();
                format!("  {}: {}", role, preview)
            })
            .collect();
        Ok(CommandOutput::Message(format!(
            "Bug report:\n  Version:    yode {}\n  OS:         {}\n  Terminal:   {}\n  Session:    {}\n  Model:      {}\n  Tokens:     {}\n\nRecent messages (last 5):\n{}",
            env!("CARGO_PKG_VERSION"),
            os_info,
            ctx.terminal_caps.summary(),
            session_short,
            ctx.session.model,
            ctx.session.total_tokens,
            recent_msgs.into_iter().rev().collect::<Vec<_>>().join("\n")
        )))
    }
}
