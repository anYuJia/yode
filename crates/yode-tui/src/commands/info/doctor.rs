use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use yode_core::updater::{latest_local_release_tag, release_version_matches_tag, CURRENT_VERSION};

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
        if args.trim() == "remote" {
            return Ok(CommandOutput::Message(render_remote_env_check(ctx)));
        }
        if args.trim() == "remote-review" {
            return Ok(CommandOutput::Message(render_remote_review_prereqs(ctx)));
        }
        if args.trim() == "remote-artifacts" {
            return Ok(CommandOutput::Message(render_remote_artifact_index(ctx)));
        }

        let mut checks = Vec::new();
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| {
                (
                    engine.runtime_state(),
                    engine.permissions().mode(),
                    engine
                        .permissions()
                        .confirmable_tools()
                        .into_iter()
                        .map(|tool| tool.to_string())
                        .collect::<Vec<_>>(),
                )
            });
        let project_root = std::path::PathBuf::from(&ctx.session.working_dir);

        // 1. Check providers
        if ctx.all_provider_models.is_empty() {
            checks.push(
                "  [!!] No LLM providers configured. Run /provider add to set one up.".to_string(),
            );
        } else {
            let names: Vec<_> = ctx.all_provider_models.keys().cloned().collect();
            checks.push(format!(
                "  [ok] LLM providers configured: {}",
                names.join(", ")
            ));
        }

        // 2. Check git
        let git_v = std::process::Command::new("git").arg("--version").output();
        match git_v {
            Ok(o) if o.status.success() => {
                let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                checks.push(format!("  [ok] git available: {}", v));
            }
            _ => checks.push("  [!!] git not found or failed".to_string()),
        }

        // 3. Check optional runtimes (Node, Python, Go, Rust)
        let runtimes = [
            ("node", "--version"),
            ("python3", "--version"),
            ("go", "version"),
            ("cargo", "--version"),
        ];

        for (cmd, arg) in runtimes {
            let output = std::process::Command::new(cmd).arg(arg).output();
            match output {
                Ok(o) if o.status.success() => {
                    let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    checks.push(format!("  [ok] {} available: {}", cmd, v));
                }
                _ => checks.push(format!("  [--] {} not found (optional)", cmd)),
            }
        }

        // 4. Check terminal capabilities
        checks.push(if ctx.terminal_caps.truecolor {
            "  [ok] Truecolor support enabled".to_string()
        } else {
            "  [--] No truecolor (using 256 colors)".to_string()
        });

        if ctx.terminal_caps.in_tmux {
            checks.push("  [--] Running inside tmux".to_string());
        }
        if ctx.terminal_caps.in_ssh {
            checks.push("  [--] Running over SSH".to_string());
        }

        // 5. Check Yode internals
        let tool_count = ctx.tools.definitions().len();
        checks.push(format!("  [ok] {} tools registered", tool_count));

        let config_path = dirs::home_dir().map(|h| h.join(".yode/config.toml"));
        if let Some(p) = config_path {
            if p.exists() {
                checks.push(format!("  [ok] Config file: {:?}", p));
            } else {
                checks.push("  [!!] Config file missing".to_string());
            }
        }

        // 6. Check context/memory runtime health
        if let Some((state, permission_mode, confirmable_tools)) = runtime {
            checks.push(format!(
                "  [ok] Compact count: {} (auto {}, manual {})",
                state.total_compactions, state.auto_compactions, state.manual_compactions
            ));
            if state.autocompact_disabled {
                checks.push(format!(
                    "  [!!] Autocompact breaker open: {}",
                    state
                        .last_compaction_breaker_reason
                        .as_deref()
                        .unwrap_or("unknown reason")
                ));
            } else {
                checks.push("  [ok] Autocompact breaker closed".to_string());
            }

            let live_path = yode_core::session_memory::live_session_memory_path(&project_root);
            if live_path.exists() {
                checks.push(format!(
                    "  [ok] Live memory file present: {}",
                    live_path.display()
                ));
            } else {
                checks.push(format!(
                    "  [--] Live memory file missing: {}",
                    live_path.display()
                ));
            }

            let session_path = yode_core::session_memory::session_memory_path(&project_root);
            if session_path.exists() {
                checks.push(format!(
                    "  [ok] Session memory file present: {}",
                    session_path.display()
                ));
            } else {
                checks.push(format!(
                    "  [--] Session memory file missing: {}",
                    session_path.display()
                ));
            }

            let transcripts_dir = project_root.join(".yode").join("transcripts");
            let transcript_count = std::fs::read_dir(&transcripts_dir)
                .ok()
                .into_iter()
                .flat_map(|entries| entries.filter_map(Result::ok))
                .count();
            checks.push(format!(
                "  [ok] Transcript artifacts visible: {}",
                transcript_count
            ));
            checks.push(format!(
                "  [ok] Session memory updates recorded: {}",
                state.session_memory_update_count
            ));
            checks.push(format!(
                "  [ok] Failed tool results tracked: {}",
                state.tracked_failed_tool_results
            ));
            checks.push(format!(
                "  [ok] Tool progress events tracked: {}",
                state.tool_progress_event_count
            ));
            checks.push(format!(
                "  [ok] Parallel tool batches tracked: {}",
                state.parallel_tool_batch_count
            ));
            if state.tool_truncation_count > 0 {
                checks.push(format!(
                    "  [!!] Tool truncations observed: {} (last: {})",
                    state.tool_truncation_count,
                    state
                        .last_tool_truncation_reason
                        .as_deref()
                        .unwrap_or("unknown")
                ));
            } else {
                checks.push("  [ok] No tool truncations observed".to_string());
            }
            if let Some(pattern) = state.latest_repeated_tool_failure.as_deref() {
                checks.push(format!("  [!!] Repeated tool failure pattern: {}", pattern));
            } else {
                checks.push("  [ok] No repeated tool failure pattern observed".to_string());
            }
            if let Some(path) = state.last_tool_turn_artifact_path.as_deref() {
                checks.push(format!("  [ok] Tool artifact available: {}", path));
            } else {
                checks.push("  [--] Tool artifact not written yet".to_string());
            }
            checks.push(format!(
                "  [ok] Hook executions tracked: {}",
                state.hook_total_executions
            ));
            if state.recovery_state != "Normal" {
                checks.push(format!(
                    "  [!!] Recovery state active: {} (signature: {})",
                    state.recovery_state,
                    state.last_failed_signature.as_deref().unwrap_or("none")
                ));
            } else {
                checks.push("  [ok] Recovery state normal".to_string());
            }
            if state.hook_timeout_count > 0 {
                checks.push(format!(
                    "  [!!] Hook timeouts observed: {} (last: {})",
                    state.hook_timeout_count,
                    state
                        .last_hook_timeout_command
                        .as_deref()
                        .unwrap_or("unknown")
                ));
            } else {
                checks.push("  [ok] No hook timeouts observed".to_string());
            }

            if matches!(permission_mode, yode_core::PermissionMode::Bypass) {
                checks.push("  [!!] Permission mode is bypass — destructive tools are fully unlocked".to_string());
            } else {
                checks.push(format!("  [ok] Permission mode: {}", permission_mode));
            }

            for critical_tool in ["bash", "write_file", "edit_file"] {
                if confirmable_tools.iter().any(|tool| tool == critical_tool) {
                    checks.push(format!(
                        "  [ok] {} still requires confirmation",
                        critical_tool
                    ));
                } else {
                    checks.push(format!(
                        "  [!!] {} no longer requires confirmation",
                        critical_tool
                    ));
                }
            }
        } else {
            checks.push("  [--] Engine runtime busy; skipped context/memory checks".to_string());
        }

        match latest_local_release_tag() {
            Some(tag) if release_version_matches_tag(&tag, CURRENT_VERSION) => {
                checks.push(format!(
                    "  [ok] Version matches latest local tag: {} == {}",
                    CURRENT_VERSION, tag
                ));
            }
            Some(tag) => {
                checks.push(format!(
                    "  [!!] Version/tag mismatch: Cargo={} latest-tag={}",
                    CURRENT_VERSION, tag
                ));
            }
            None => {
                checks.push("  [--] Could not determine latest local release tag".to_string());
            }
        }

        Ok(CommandOutput::Message(format!(
            "Yode Environment Health Check:\n\n{}\n\n  Platform: {} {}\n  Version:  v{}\n  Session:  {}",
            checks.join("\n"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            env!("CARGO_PKG_VERSION"),
            &ctx.session.session_id[..8],
        )))
    }
}

fn render_remote_env_check(ctx: &mut CommandContext) -> String {
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
        Ok(path) => checks.push(format!("  [ok] Current yode executable: {}", path.display())),
        Err(err) => checks.push(format!("  [!!] Could not resolve current executable: {}", err)),
    }

    format!(
        "Remote Environment Verification:\n\n{}\n\nNext steps:\n  Use this before launching remote review/worktree flows.\n  Fix [!!] items before relying on remote execution.",
        checks.join("\n")
    )
}

fn render_remote_review_prereqs(ctx: &mut CommandContext) -> String {
    let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
    let mut checks = Vec::new();

    checks.push(format!(
        "  [{}] Provider models available: {}",
        if ctx.all_provider_models.is_empty() { "!!" } else { "ok" },
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
        .map(|def| def.name)
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
        if ctx.terminal_caps.in_ssh { "ssh" } else { "local" }
    ));

    format!(
        "Remote Review Prerequisites:\n\n{}\n\nNext steps:\n  Use `/doctor remote` for base transport checks.\n  Fix [!!] items before relying on remote review automation.",
        checks.join("\n")
    )
}

fn render_remote_artifact_index(ctx: &mut CommandContext) -> String {
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
        let meta = std::fs::metadata(&path).ok();
        let size = meta.as_ref().map(|meta| meta.len()).unwrap_or(0);
        let modified = meta
            .and_then(|meta| meta.modified().ok())
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

fn ssh_context_label(ssh_tty: Option<&str>, ssh_connection: Option<&str>) -> &'static str {
    if ssh_tty.is_some() || ssh_connection.is_some() {
        "ssh"
    } else {
        "local"
    }
}

#[cfg(test)]
mod tests {
    use super::ssh_context_label;

    #[test]
    fn ssh_context_label_detects_remote_env() {
        assert_eq!(ssh_context_label(Some("/dev/ttys001"), None), "ssh");
        assert_eq!(ssh_context_label(None, Some("client server 22")), "ssh");
        assert_eq!(ssh_context_label(None, None), "local");
    }
}
