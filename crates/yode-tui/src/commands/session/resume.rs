use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct ResumeCommand {
    meta: CommandMeta,
}

impl ResumeCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "resume",
                description: "Show or prepare a session resume command",
                aliases: &[],
                args: Vec::new(),
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for ResumeCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let session_id = args.trim();
        if session_id.is_empty() {
            let current = render_resume_command(&ctx.session.session_id);
            let recent = match super::SessionsCommand::new().execute("", ctx) {
                Ok(CommandOutput::Message(message)) => message,
                Ok(_) => "Recent sessions unavailable.".to_string(),
                Err(err) => format!("Recent sessions unavailable: {}", err),
            };
            return Ok(CommandOutput::Message(render_resume_overview(
                &current, &recent,
            )));
        }

        let command = render_resume_command(session_id);
        Ok(CommandOutput::Message(format!(
            "Resume this session from a new shell with:\n  {}",
            command
        )))
    }
}

fn render_resume_command(session_id: &str) -> String {
    format!("yode --resume {}", shell_quote(session_id))
}

fn render_resume_overview(current_command: &str, recent_sessions: &str) -> String {
    format!(
        "Current session:\n  {}\n\n{}",
        current_command, recent_sessions
    )
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/' | '@'))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::{render_resume_command, render_resume_overview};

    #[test]
    fn resume_command_keeps_simple_session_ids_readable() {
        assert_eq!(
            render_resume_command("abc12345-def"),
            "yode --resume abc12345-def"
        );
    }

    #[test]
    fn resume_command_quotes_shell_sensitive_session_ids() {
        assert_eq!(render_resume_command("abc 123"), "yode --resume 'abc 123'");
        assert_eq!(
            render_resume_command("abc'123"),
            "yode --resume 'abc'\\''123'"
        );
    }

    #[test]
    fn resume_overview_always_includes_current_command() {
        let rendered = render_resume_overview(
            "yode --resume current-session",
            "Recent sessions unavailable: db locked",
        );

        assert!(rendered.contains("yode --resume current-session"));
        assert!(rendered.contains("Recent sessions unavailable: db locked"));
    }
}
