use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

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
                args: vec![],
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

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let mut checks = Vec::new();
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_state());
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
        if let Some(state) = runtime {
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
        } else {
            checks.push("  [--] Engine runtime busy; skipped context/memory checks".to_string());
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
