use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use crate::commands::info::permission_recovery_workspace::render_hook_workspace;
use crate::runtime_artifacts::write_hook_failure_artifact;

pub struct HooksCommand {
    meta: CommandMeta,
}

impl HooksCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "hooks",
                description: "Show hook runtime diagnostics",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for HooksCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_state());

        let Some(state) = runtime else {
            return Ok(CommandOutput::Message(
                "Hook diagnostics unavailable: engine busy.".to_string(),
            ));
        };
        let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
        let hook_artifact =
            write_hook_failure_artifact(&project_root, &ctx.session.session_id, &state);

        Ok(CommandOutput::Message(render_hook_workspace(
            &state,
            hook_artifact.as_deref(),
        )))
    }
}
