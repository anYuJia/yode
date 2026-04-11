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

pub(super) fn export_doctor_bundle(ctx: &mut CommandContext) -> Result<String, String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let bundle_dir = cwd.join(format!(
        "doctor-bundle-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::create_dir_all(&bundle_dir)
        .map_err(|err| format!("Failed to create doctor bundle dir: {}", err))?;

    let reports = [
        ("local-doctor.txt", local::render_doctor_report(ctx)),
        ("remote-env.txt", remote::render_remote_env_check(ctx)),
        ("remote-review.txt", remote::render_remote_review_prereqs(ctx)),
        ("remote-artifacts.txt", remote::render_remote_artifact_index(ctx)),
    ];

    for (name, body) in reports {
        let path = bundle_dir.join(name);
        std::fs::write(&path, body)
            .map_err(|err| format!("Failed to write {}: {}", path.display(), err))?;
    }

    Ok(format!(
        "Doctor bundle exported to: {}\n  Files: local-doctor.txt, remote-env.txt, remote-review.txt, remote-artifacts.txt",
        bundle_dir.display()
    ))
}

#[cfg(test)]
pub(super) fn ssh_context_label(
    ssh_tty: Option<&str>,
    ssh_connection: Option<&str>,
) -> &'static str {
    remote::ssh_context_label(ssh_tty, ssh_connection)
}
