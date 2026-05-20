use super::diagnostics_render::render_diagnostics_overview_with_width;
use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct DiagnosticsCommand {
    meta: CommandMeta,
}

impl DiagnosticsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "diagnostics",
                description: "Show a unified diagnostics overview",
                aliases: &["diag"],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for DiagnosticsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| (engine.runtime_state(), engine.runtime_tasks_snapshot()));
        let Some((state, tasks)) = runtime else {
            return Ok(CommandOutput::Message(
                "Diagnostics unavailable: engine busy.".to_string(),
            ));
        };

        Ok(CommandOutput::Message(
            render_diagnostics_overview_with_width(
                std::path::Path::new(&ctx.session.working_dir),
                &state,
                &tasks,
                diagnostics_terminal_width(),
            ),
        ))
    }
}

fn diagnostics_terminal_width() -> usize {
    crossterm::terminal::size()
        .ok()
        .map(|(width, _)| width as usize)
        .unwrap_or(96)
}
