use crate::commands::context::CommandContext;

pub(super) fn render_remote_env_check(ctx: &mut CommandContext) -> String {
    let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
    let mut checks = Vec::new();

    checks.push(format!("  [ok] Working dir: {}", project_root.display()));
    checks.push(format!(
        "  [{}] SSH context: {}",
        if ctx.terminal_caps.in_ssh { "ok" } else { "--" },
        ssh_context_label(
            std::env::var("SSH_TTY").ok().as_deref(),
            std::env::var("SSH_CONNECTION").ok().as_deref(),
        )
    ));

    for (command, version_arg) in [("ssh", "-V"), ("git", "--version"), ("sh", "--version")] {
        if command_available(command, version_arg) {
            checks.push(format!("  [ok] {} available", command));
        } else {
            checks.push(format!("  [!!] {} not found in PATH", command));
        }
    }
    checks.push(if command_available("rsync", "--version") {
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
    checks.push(match origin {
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
        Ok(()) => checks.push(format!(
            "  [ok] Remote artifact dir writable: {}",
            remote_dir.display()
        )),
        Err(err) => checks.push(format!(
            "  [!!] Remote artifact dir not writable: {} ({})",
            remote_dir.display(),
            err
        )),
    }

    match std::env::current_exe() {
        Ok(path) => checks.push(format!(
            "  [ok] Current yode executable: {}",
            path.display()
        )),
        Err(err) => checks.push(format!(
            "  [!!] Could not resolve current executable: {}",
            err
        )),
    }

    format!(
        "Remote Environment Verification:\n\n{}\n\nNext steps:\n  Use this before launching remote review/worktree flows.\n  Fix [!!] items before relying on remote execution.",
        checks.join("\n")
    )
}

pub(super) fn render_remote_review_prereqs(ctx: &mut CommandContext) -> String {
    let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
    let mut checks = Vec::new();

    checks.push(format!(
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

    checks.push(if project_root.join(".git").exists() {
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
    checks.push(match git_status {
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
        Ok(()) => checks.push(format!(
            "  [ok] Review artifact dir writable: {}",
            review_dir.display()
        )),
        Err(err) => checks.push(format!(
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
        checks.push(if tool_names.contains(tool_name) {
            format!("  [ok] {} tool registered", tool_name)
        } else {
            format!("  [!!] {} tool missing", tool_name)
        });
    }

    checks.push(format!(
        "  [{}] Terminal transport: {}",
        if ctx.terminal_caps.in_ssh { "ok" } else { "--" },
        if ctx.terminal_caps.in_ssh {
            "ssh"
        } else {
            "local"
        }
    ));

    format!(
        "Remote Review Prerequisites:\n\n{}\n\nNext steps:\n  Use `/doctor remote` for base transport checks.\n  Fix [!!] items before relying on remote review automation.",
        checks.join("\n")
    )
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
        return format!(
            "Remote Session Artifact Index:\n\n  [--] No remote artifacts found in {}\n\nNext steps:\n  Run `/doctor remote` first to verify the base remote workspace path.",
            remote_dir.display()
        );
    }

    let mut lines = vec![format!(
        "Remote Session Artifact Index:\n\n  Directory: {}\n  Files:     {}",
        remote_dir.display(),
        entries.len()
    )];
    for path in entries.into_iter().take(20) {
        let metadata = std::fs::metadata(&path).ok();
        let size = metadata
            .as_ref()
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let modified = metadata
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|stamp| stamp.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|stamp| stamp.as_secs().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        lines.push(format!(
            "  - {} ({} bytes, mtime={})",
            path.display(),
            size,
            modified
        ));
    }
    lines.join("\n")
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
