mod local;
mod remote;

use crate::commands::context::CommandContext;

pub(super) fn render_doctor_report(ctx: &mut CommandContext) -> String {
    local::render_doctor_report(ctx)
}

pub(super) fn render_remote_env_check(ctx: &mut CommandContext) -> String {
    remote::render_remote_env_check(ctx)
}

pub(super) fn render_remote_review_prereqs(ctx: &mut CommandContext) -> String {
    remote::render_remote_review_prereqs(ctx)
}

pub(super) fn render_remote_artifact_index(ctx: &mut CommandContext) -> String {
    remote::render_remote_artifact_index(ctx)
}

#[cfg(test)]
pub(super) fn ssh_context_label(
    ssh_tty: Option<&str>,
    ssh_connection: Option<&str>,
) -> &'static str {
    remote::ssh_context_label(ssh_tty, ssh_connection)
}
