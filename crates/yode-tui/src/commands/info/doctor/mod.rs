mod report;
#[cfg(test)]
mod tests;

use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

use self::report::{
    export_doctor_bundle, render_doctor_report, render_remote_artifact_index,
    render_remote_control_doctor, render_remote_env_check, render_remote_review_prereqs,
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
                    hint: "[remote|remote-review|remote-artifacts|bundle]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "bundle".to_string(),
                        "remote".to_string(),
                        "remote-control".to_string(),
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
            "bundle" => export_doctor_bundle(ctx)?,
            "remote" => render_remote_env_check(ctx),
            "remote-control" => render_remote_control_doctor(ctx),
            "remote-review" => render_remote_review_prereqs(ctx),
            "remote-artifacts" => render_remote_artifact_index(ctx),
            _ => render_doctor_report(ctx),
        };
        Ok(CommandOutput::Message(message))
    }
}
