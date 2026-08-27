use super::*;

use super::parsing::parse_structured_hook_output_result;

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

        let result = self
            .run_hook_command(&hook.command, &context_json, &context.event, timeout)
            .await;

        match result {
            HookCommandResult::Output(output) => {
                let stdout = normalize_hook_output(&output.stdout);
                let stderr = normalize_hook_output(&output.stderr);
                let mut structured = match parse_structured_hook_output_result(&stdout) {
                    Ok(parsed) => parsed,
                    Err(message) => {
                        let reason = format!("invalid structured hook output: {message}");
                        tracing::warn!(command = %hook.command, event = %context.event, "{reason}");
                        self.record_hook_failure(&context.event, &hook.command, reason, false);
                        None
                    }
                };
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
            HookCommandResult::Io(e) => {
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
            HookCommandResult::Timeout => {
                tracing::warn!(
                    "Hook '{}' timed out after {}s (event={}); process group terminated",
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
        timeout: std::time::Duration,
    ) -> HookCommandResult {
        #[cfg(not(windows))]
        let mut cmd = self.build_hook_command("sh", &["-c"], command, context_json, event);
        #[cfg(windows)]
        let mut cmd =
            self.build_hook_command("cmd.exe", &["/d", "/s", "/c"], command, context_json, event);
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        // 独立进程组：超时/取消时能连同全部后代进程一起终止并回收。
        yode_tools::process_env::spawn_in_new_process_group(&mut cmd);
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => return HookCommandResult::Io(err),
        };
        let drain = drain_pipe(child.stdout.take(), child.stderr.take());
        let wait = child.wait();

        let status = tokio::select! {
            status = wait => match status {
                Ok(status) => status,
                Err(err) => return HookCommandResult::Io(err),
            },
            _ = tokio::time::sleep(timeout) => {
                // BUG-005/CANCEL-001：超时后必须终止整个进程组并回收，
                // 不允许脚本继续运行（安全策略 fail-closed 的进程侧保证）。
                let _ = yode_tools::process_env::kill_process_group(&mut child).await;
                return HookCommandResult::Timeout;
            }
        };
        let (stdout, stderr) = drain.await;
        HookCommandResult::Output(std::process::Output {
            status,
            stdout,
            stderr,
        })
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
        // 仓库可控的 hook 脚本不得继承父进程环境：父进程的 API key、
        // 凭据、代理设置等只能通过明确授权的方式传递。
        yode_tools::process_env::apply_minimal_env(&mut process);
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
    let decoded = decode_utf16_le_if_needed(bytes)
        .unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned());
    decoded.replace("\r\n", "\n")
}

/// Windows PowerShell 5 can emit redirected native output as UTF-16LE. Detect a BOM or the
/// characteristic NUL high bytes of ASCII-heavy JSON/error text before falling back to UTF-8.
fn decode_utf16_le_if_needed(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 {
        return None;
    }
    let (payload, has_bom) = if bytes.starts_with(&[0xff, 0xfe]) {
        (&bytes[2..], true)
    } else {
        (bytes, false)
    };
    if payload.len() % 2 != 0 {
        return None;
    }

    if !has_bom {
        let mut sampled = 0usize;
        let mut zero_high = 0usize;
        for pair in payload.chunks_exact(2).take(32) {
            sampled += 1;
            if pair[1] == 0 && pair[0] != 0 {
                zero_high += 1;
            }
        }
        if sampled < 2 || zero_high * 2 < sampled {
            return None;
        }
    }

    let units = payload
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    Some(String::from_utf16_lossy(&units))
}

enum HookCommandResult {
    Output(std::process::Output),
    Io(std::io::Error),
    Timeout,
}

/// 并行排空子进程管道，避免 stdout/stderr 满时死锁。
async fn drain_pipe(
    pipe: Option<tokio::process::ChildStdout>,
    stderr_pipe: Option<tokio::process::ChildStderr>,
) -> (Vec<u8>, Vec<u8>) {
    use tokio::io::AsyncReadExt;

    async fn read_stdout(mut pipe: Option<tokio::process::ChildStdout>) -> Vec<u8> {
        let mut output = Vec::new();
        if let Some(pipe) = pipe.as_mut() {
            let _ = pipe.read_to_end(&mut output).await;
        }
        output
    }

    async fn read_stderr(mut pipe: Option<tokio::process::ChildStderr>) -> Vec<u8> {
        let mut output = Vec::new();
        if let Some(pipe) = pipe.as_mut() {
            let _ = pipe.read_to_end(&mut output).await;
        }
        output
    }

    tokio::join!(read_stdout(pipe), read_stderr(stderr_pipe))
}

#[cfg(test)]
mod output_encoding_tests {
    use super::*;

    #[test]
    fn utf8_hook_output_is_unchanged_except_crlf() {
        assert_eq!(
            normalize_hook_output(b"{\"ok\":true}\r\n"),
            "{\"ok\":true}\n"
        );
    }

    #[test]
    fn utf16le_hook_output_is_decoded_and_normalized() {
        let mut bytes = Vec::new();
        for unit in "{\"decision\":\"defer\"}\r\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(normalize_hook_output(&bytes), "{\"decision\":\"defer\"}\n");
    }
}
