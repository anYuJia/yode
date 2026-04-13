use crate::commands::context::CommandContext;
use crate::commands::workspace_text::{workspace_bullets, WorkspaceText};
use super::remote_workspace::{
    browser_capability_checklist, build_remote_workflow_state,
    remote_command_surface_inventory, remote_missing_prereq_summary,
    remote_prereq_severity_banner, render_remote_capability_workspace,
    write_remote_workflow_capability_artifact,
};
use super::shared::format_artifact_entry;

pub(super) fn render_remote_env_check(ctx: &mut CommandContext) -> String {
    let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
    let workflow_state = build_remote_workflow_state(ctx);
    let mut transport_checks = Vec::new();
    let mut repo_checks = Vec::new();
    let mut artifact_checks = Vec::new();

    repo_checks.push(format!("  [ok] Working dir: {}", project_root.display()));
    transport_checks.push(format!(
        "  [{}] SSH context: {}",
        if ctx.terminal_caps.in_ssh { "ok" } else { "--" },
        ssh_context_label(
            std::env::var("SSH_TTY").ok().as_deref(),
            std::env::var("SSH_CONNECTION").ok().as_deref(),
        )
    ));

    for (command, version_arg) in [("ssh", "-V"), ("git", "--version"), ("sh", "--version")] {
        if command_available(command, version_arg) {
            transport_checks.push(format!("  [ok] {} available", command));
        } else {
            transport_checks.push(format!("  [!!] {} not found in PATH", command));
        }
    }
    transport_checks.push(if command_available("rsync", "--version") {
        "  [ok] rsync available".to_string()
    } else {
        "  [--] rsync not found (optional; scp/tar fallback may still work)".to_string()
    });

    let origin = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(&project_root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty());
    repo_checks.push(match origin {
        Some(origin) => format!("  [ok] Git origin remote: {}", origin),
        None => "  [--] Git origin remote not configured".to_string(),
    });

    let remote_dir = project_root.join(".yode").join("remote");
    match std::fs::create_dir_all(&remote_dir).and_then(|_| {
        let probe = remote_dir.join(".remote-env-check");
        std::fs::write(&probe, b"ok")?;
        std::fs::remove_file(probe)?;
        Ok(())
    }) {
        Ok(()) => artifact_checks.push(format!(
            "  [ok] Remote artifact dir writable: {}",
            remote_dir.display()
        )),
        Err(err) => artifact_checks.push(format!(
            "  [!!] Remote artifact dir not writable: {} ({})",
            remote_dir.display(),
            err
        )),
    }

    match std::env::current_exe() {
        Ok(path) => transport_checks.push(format!(
            "  [ok] Current yode executable: {}",
            path.display()
        )),
        Err(err) => transport_checks.push(format!(
            "  [!!] Could not resolve current executable: {}",
            err
        )),
    }
    let command_inventory = remote_command_surface_inventory();
    let capability_artifact = write_remote_workflow_capability_artifact(
        &project_root,
        &ctx.session.session_id,
        &workflow_state,
        &command_inventory,
    );
    if let Some(path) = capability_artifact.as_deref() {
        artifact_checks.push(format!("  [ok] Remote capability artifact: {}", path));
    }
    artifact_checks.extend(browser_capability_checklist(ctx));

    WorkspaceText::new("Remote environment workspace")
        .subtitle(project_root.display().to_string())
        .field("Command surface", command_inventory)
        .field("Missing prereqs", remote_missing_prereq_summary(&workflow_state))
        .field("Severity", remote_prereq_severity_banner(&workflow_state))
        .section("Transport", workspace_bullets(transport_checks))
        .section("Repository", workspace_bullets(repo_checks))
        .section("Artifacts", workspace_bullets(artifact_checks))
        .section(
            "Capability preview",
            capability_artifact
                .as_deref()
                .and_then(|path| render_remote_capability_workspace(std::path::Path::new(path)))
                .map(|preview| workspace_bullets([preview]))
                .unwrap_or_else(|| workspace_bullets(["none"])),
        )
        .footer("Use this before launching remote review/worktree flows.")
        .render()
}

pub(super) fn render_remote_review_prereqs(ctx: &mut CommandContext) -> String {
    let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
    let workflow_state = build_remote_workflow_state(ctx);
    let mut provider_checks = Vec::new();
    let mut repo_checks = Vec::new();
    let mut tool_checks = Vec::new();
    let mut artifact_checks = Vec::new();

    provider_checks.push(format!(
        "  [{}] Provider models available: {}",
        if ctx.all_provider_models.is_empty() {
            "!!"
        } else {
            "ok"
        },
        if ctx.all_provider_models.is_empty() {
            "none".to_string()
        } else {
            ctx.all_provider_models
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        }
    ));

    repo_checks.push(if project_root.join(".git").exists() {
        format!("  [ok] Git repo detected: {}", project_root.display())
    } else {
        format!("  [!!] Not a git repo: {}", project_root.display())
    });

    let git_status = std::process::Command::new("git")
        .args(["status", "--short"])
        .current_dir(&project_root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());
    repo_checks.push(match git_status {
        Some(status) if status.is_empty() => "  [ok] Working tree clean".to_string(),
        Some(status) => format!(
            "  [--] Working tree has changes (remote review still possible): {}",
            status.lines().take(3).collect::<Vec<_>>().join(" | ")
        ),
        None => "  [!!] Could not read git status".to_string(),
    });

    let review_dir = project_root.join(".yode").join("reviews");
    match std::fs::create_dir_all(&review_dir).and_then(|_| {
        let probe = review_dir.join(".remote-review-check");
        std::fs::write(&probe, b"ok")?;
        std::fs::remove_file(probe)?;
        Ok(())
    }) {
        Ok(()) => artifact_checks.push(format!(
            "  [ok] Review artifact dir writable: {}",
            review_dir.display()
        )),
        Err(err) => artifact_checks.push(format!(
            "  [!!] Review artifact dir not writable: {} ({})",
            review_dir.display(),
            err
        )),
    }

    let tool_names = ctx
        .tools
        .definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<std::collections::BTreeSet<_>>();
    for tool_name in ["review_changes", "review_pipeline", "review_then_commit"] {
        tool_checks.push(if tool_names.contains(tool_name) {
            format!("  [ok] {} tool registered", tool_name)
        } else {
            format!("  [!!] {} tool missing", tool_name)
        });
    }

    provider_checks.push(format!(
        "  [{}] Terminal transport: {}",
        if ctx.terminal_caps.in_ssh { "ok" } else { "--" },
        if ctx.terminal_caps.in_ssh {
            "ssh"
        } else {
            "local"
        }
    ));
    tool_checks.extend(browser_capability_checklist(ctx));

    WorkspaceText::new("Remote review workspace")
        .subtitle(project_root.display().to_string())
        .field("Missing prereqs", remote_missing_prereq_summary(&workflow_state))
        .field("Severity", remote_prereq_severity_banner(&workflow_state))
        .section("Provider", workspace_bullets(provider_checks))
        .section("Repository", workspace_bullets(repo_checks))
        .section("Tools", workspace_bullets(tool_checks))
        .section("Artifacts", workspace_bullets(artifact_checks))
        .footer("Use `/doctor remote` for base transport checks.")
        .render()
}

pub(super) fn render_remote_artifact_index(ctx: &mut CommandContext) -> String {
    let remote_dir = std::path::PathBuf::from(&ctx.session.working_dir)
        .join(".yode")
        .join("remote");
    let mut entries = std::fs::read_dir(&remote_dir)
        .ok()
        .into_iter()
        .flat_map(|iter| iter.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    entries.sort();

    if entries.is_empty() {
        return WorkspaceText::new("Remote artifact workspace")
            .subtitle(remote_dir.display().to_string())
            .section(
                "Artifacts",
                workspace_bullets([format!(
                    "[--] No remote artifacts found in {}",
                    remote_dir.display()
                )]),
            )
            .footer("Run `/doctor remote` first to verify the base remote workspace path.")
            .render();
    }

    let total = entries.len();
    let mut artifact_lines = entries
        .into_iter()
        .take(12)
        .map(|path| format_artifact_entry(&path))
        .collect::<Vec<_>>();
    if total > 12 {
        artifact_lines.push("... artifact index folded ...".to_string());
    }
    WorkspaceText::new("Remote artifact workspace")
        .subtitle(remote_dir.display().to_string())
        .field("Files", total.to_string())
        .section("Artifacts", workspace_bullets(artifact_lines))
        .render()
}

fn command_available(command: &str, version_arg: &str) -> bool {
    std::process::Command::new(command)
        .arg(version_arg)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(super) fn ssh_context_label(
    ssh_tty: Option<&str>,
    ssh_connection: Option<&str>,
) -> &'static str {
    if ssh_tty.is_some() || ssh_connection.is_some() {
        "ssh"
    } else {
        "local"
    }
}
