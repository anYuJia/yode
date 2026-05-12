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
            let recent = super::SessionsCommand::new().execute("", ctx)?;
            return Ok(match recent {
                CommandOutput::Message(message) => CommandOutput::Message(format!(
                    "Current session:\n  {}\n\n{}",
                    current, message
                )),
                other => other,
            });
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
    use super::render_resume_command;

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
}
