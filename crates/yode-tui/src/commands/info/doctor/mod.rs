mod report;
#[cfg(test)]
mod tests;

use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

use self::report::{
    render_doctor_report, render_remote_artifact_index, render_remote_env_check,
    render_remote_review_prereqs,
};

pub struct DoctorCommand {
    meta: CommandMeta,
}

impl DoctorCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "doctor",
                description: "Run environment health check",
                aliases: &[],
                args: vec![ArgDef {
                    name: "target".to_string(),
                    required: false,
                    hint: "[remote|remote-review|remote-artifacts]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "remote".to_string(),
                        "remote-review".to_string(),
                        "remote-artifacts".to_string(),
                    ]),
                }],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for DoctorCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let message = match args.trim() {
            "remote" => render_remote_env_check(ctx),
            "remote-review" => render_remote_review_prereqs(ctx),
            "remote-artifacts" => render_remote_artifact_index(ctx),
            _ => render_doctor_report(ctx),
        };
        Ok(CommandOutput::Message(message))
    }
}
