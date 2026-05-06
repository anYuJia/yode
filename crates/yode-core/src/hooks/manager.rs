use super::*;

use super::parsing::parse_structured_hook_output;

impl HookManager {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            hooks: Vec::new(),
            working_dir,
            wake_notifications: Mutex::new(Vec::new()),
            stats: Mutex::new(HookManagerStats::default()),
        }
    }

    pub fn register(&mut self, hook: HookDefinition) {
        self.hooks.push(hook);
    }

    pub fn register_all(&mut self, hooks: Vec<HookDefinition>) {
        self.hooks.extend(hooks);
    }

    /// Execute all hooks matching the given event.
    pub async fn execute(&self, event: HookEvent, context: &HookContext) -> Vec<HookResult> {
        let event_str = event.to_string();
        let matching: Vec<&HookDefinition> = self
            .hooks
            .iter()
            .filter(|h| h.events.iter().any(|e| e == &event_str))
            .filter(|h| {
                if let Some(ref filter) = h.tool_filter {
                    if let Some(ref tool_name) = context.tool_name {
                        filter.iter().any(|f| f == tool_name)
                    } else {
                        true
                    }
                } else {
                    true
                }
            })
            .collect();

        let mut results = Vec::new();

        for hook in matching {
            let mut result = self.execute_hook(hook, context).await;
            if result.source_hook_command.is_none() {
                result.source_hook_command = Some(hook.command.clone());
            }
            if result.deferred {
                self.record_hook_defer(
                    &hook.command,
                    result.reason.as_deref().unwrap_or("deferred by hook"),
                );
            }
            if let Some(message) = result.wake_notification.clone() {
                if let Ok(mut notifications) = self.wake_notifications.lock() {
                    notifications.push(WakeNotification {
                        event: context.event.clone(),
                        hook_command: hook.command.clone(),
                        message,
                    });
                }
            }
            results.push(result);
        }

        results
    }

    pub async fn check_blocked(
        &self,
        event: HookEvent,
        context: &HookContext,
    ) -> Option<HookResult> {
        let results = self.execute(event, context).await;
        results.into_iter().find(|r| r.blocked)
    }

    pub fn drain_wake_notifications(&self) -> Vec<WakeNotification> {
        if let Ok(mut notifications) = self.wake_notifications.lock() {
            std::mem::take(&mut *notifications)
        } else {
            Vec::new()
        }
    }

    pub fn stats_snapshot(&self) -> HookManagerStats {
        self.stats
            .lock()
            .map(|stats| stats.clone())
            .unwrap_or_default()
    }

    async fn execute_hook(&self, hook: &HookDefinition, context: &HookContext) -> HookResult {
        self.record_hook_attempt();
        let context_json = match serde_json::to_string(context) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("Failed to serialize hook context: {}", e);
                return HookResult::allowed();
            }
        };

        let timeout = std::time::Duration::from_secs(hook.timeout_secs);
        let started_at = Instant::now();

        let result = tokio::time::timeout(timeout, async {
            self.run_hook_command(&hook.command, &context_json, &context.event)
                .await
        })
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = normalize_hook_output(&output.stdout);
                let stderr = normalize_hook_output(&output.stderr);
                let mut structured = parse_structured_hook_output(&stdout);
                if let Some(ref mut parsed) = structured {
                    if parsed.blocked && !hook.can_block {
                        parsed.blocked = false;
                    }
                }

                if output.status.code() == Some(2) {
                    self.record_hook_wake();
                    let wake_message = structured
                        .as_ref()
                        .and_then(|parsed| parsed.wake_notification.clone())
                        .or_else(|| {
                            let trimmed = stdout.trim();
                            if !trimmed.is_empty() {
                                Some(trimmed.to_string())
                            } else {
                                let trimmed = stderr.trim();
                                if !trimmed.is_empty() {
                                    Some(trimmed.to_string())
                                } else {
                                    Some(format!(
                                        "Hook '{}' requested wake notification",
                                        hook.command
                                    ))
                                }
                            }
                        });

                    if let Some(mut parsed) = structured {
                        parsed.blocked = false;
                        parsed.wake_notification = wake_message;
                        return parsed;
                    }

                    return HookResult {
                        blocked: false,
                        deferred: false,
                        reason: None,
                        modified_input: None,
                        stdout: if stdout.is_empty() {
                            None
                        } else {
                            Some(stdout)
                        },
                        wake_notification: wake_message,
                        source_hook_command: None,
                    };
                }

                if !output.status.success() && hook.can_block {
                    self.record_hook_failure(
                        &context.event,
                        &hook.command,
                        if stderr.is_empty() {
                            format!(
                                "non-zero exit after {}ms: {}",
                                started_at.elapsed().as_millis(),
                                output.status
                            )
                        } else {
                            format!(
                                "non-zero exit after {}ms: {}",
                                started_at.elapsed().as_millis(),
                                stderr.trim()
                            )
                        },
                        true,
                    );
                    if let Some(mut parsed) = structured {
                        if parsed.reason.is_none() {
                            parsed.reason = Some(if stderr.is_empty() {
                                format!(
                                    "Hook '{}' exited with code {}",
                                    hook.command, output.status
                                )
                            } else {
                                stderr.trim().to_string()
                            });
                        }
                        parsed.blocked = true;
                        parsed
                    } else {
                        HookResult {
                            blocked: true,
                            deferred: false,
                            reason: Some(if stderr.is_empty() {
                                format!(
                                    "Hook '{}' exited with code {}",
                                    hook.command, output.status
                                )
                            } else {
                                stderr.trim().to_string()
                            }),
                            modified_input: None,
                            stdout: Some(stdout),
                            wake_notification: None,
                            source_hook_command: None,
                        }
                    }
                } else if let Some(parsed) = structured {
                    parsed
                } else {
                    HookResult {
                        blocked: false,
                        deferred: false,
                        reason: None,
                        modified_input: None,
                        stdout: if stdout.is_empty() {
                            None
                        } else {
                            Some(stdout)
                        },
                        wake_notification: None,
                        source_hook_command: None,
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("Hook execution failed: {}", e);
                self.record_hook_failure(
                    &context.event,
                    &hook.command,
                    format!(
                        "spawn/exec error after {}ms: {}",
                        started_at.elapsed().as_millis(),
                        e
                    ),
                    false,
                );
                HookResult::allowed()
            }
            Err(_) => {
                tracing::warn!(
                    "Hook '{}' timed out after {}s (event={})",
                    hook.command,
                    hook.timeout_secs,
                    context.event,
                );
                self.record_hook_timeout(&context.event, &hook.command, hook.timeout_secs);
                HookResult::allowed()
            }
        }
    }

    async fn run_hook_command(
        &self,
        command: &str,
        context_json: &str,
        event: &str,
    ) -> std::io::Result<std::process::Output> {
        #[cfg(windows)]
        {
            match self
                .build_hook_command("cmd.exe", &["/C"], command, context_json, event)
                .output()
                .await
            {
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                result => return result,
            }

            for program in [
                "bash",
                r"C:\Program Files\Git\bin\bash.exe",
                r"C:\Program Files\Git\usr\bin\bash.exe",
            ] {
                match self
                    .build_hook_command(program, &["-lc"], command, context_json, event)
                    .output()
                    .await
                {
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                    result => return result,
                }
            }

            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no supported hook shell found",
            ));
        }

        #[cfg(not(windows))]
        {
            self.build_hook_command("sh", &["-c"], command, context_json, event)
                .output()
                .await
        }
    }

    fn build_hook_command(
        &self,
        program: &str,
        shell_args: &[&str],
        command: &str,
        context_json: &str,
        event: &str,
    ) -> tokio::process::Command {
        let mut process = tokio::process::Command::new(program);
        process
            .args(shell_args)
            .arg(command)
            .env("YODE_HOOK_CONTEXT", context_json)
            .env("YODE_HOOK_EVENT", event)
            .current_dir(&self.working_dir);
        process
    }

    fn record_hook_attempt(&self) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.total_executions = stats.total_executions.saturating_add(1);
        }
    }

    fn record_hook_wake(&self) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.wake_notification_count = stats.wake_notification_count.saturating_add(1);
        }
    }

    fn record_hook_defer(&self, command: &str, reason: &str) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.defer_count = stats.defer_count.saturating_add(1);
            stats.last_defer_command = Some(command.to_string());
            stats.last_defer_reason = Some(reason.to_string());
            stats.last_defer_at =
                Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        }
    }

    fn record_hook_timeout(&self, event: &str, command: &str, timeout_secs: u64) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.timeout_count = stats.timeout_count.saturating_add(1);
            stats.last_failure_event = Some(event.to_string());
            stats.last_failure_command = Some(command.to_string());
            stats.last_failure_reason = Some(format!("timed out after {}s", timeout_secs));
            stats.last_failure_at =
                Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
            stats.last_timeout_command = Some(command.to_string());
        }
    }

    fn record_hook_failure(&self, event: &str, command: &str, reason: String, nonzero_exit: bool) {
        if let Ok(mut stats) = self.stats.lock() {
            if nonzero_exit {
                stats.nonzero_exit_count = stats.nonzero_exit_count.saturating_add(1);
            } else {
                stats.execution_error_count = stats.execution_error_count.saturating_add(1);
            }
            stats.last_failure_event = Some(event.to_string());
            stats.last_failure_command = Some(command.to_string());
            stats.last_failure_reason = Some(reason);
            stats.last_failure_at =
                Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        }
    }
}

fn normalize_hook_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}
