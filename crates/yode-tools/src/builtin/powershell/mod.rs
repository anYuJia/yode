use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::LazyLock;
#[cfg(test)]
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use tokio::process::Command;
use uuid::Uuid;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const MAX_TIMEOUT_SECS: u64 = 600;

static PS_SEARCH_COMMANDS: &[&str] = &["select-string", "findstr", "where.exe"];
static PS_READ_COMMANDS: &[&str] = &[
    "get-content",
    "get-item",
    "get-itemproperty",
    "test-path",
    "resolve-path",
    "get-process",
    "get-service",
    "get-childitem",
    "get-location",
    "get-filehash",
    "get-acl",
    "format-hex",
    "get-command",
    "get-help",
    "get-module",
    "get-alias",
];
static PS_READONLY_NAVIGATION_COMMANDS: &[&str] = &["set-location", "push-location", "pop-location"];
static PS_GIT_READONLY_SUBCOMMANDS: &[&str] = &["status", "diff", "log", "show", "rev-parse"];

static DESTRUCTIVE_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (
            Regex::new(r"(?i)\b(remove-item|rm|del|rd|rmdir|ri)\b.*\-(recurse|force)").unwrap(),
            "Note: may recursively or forcibly remove files",
        ),
        (
            Regex::new(r"(?i)\bformat-volume\b").unwrap(),
            "Note: may format a disk volume",
        ),
        (
            Regex::new(r"(?i)\bclear-disk\b").unwrap(),
            "Note: may clear a disk",
        ),
        (
            Regex::new(r"(?i)\bstop-computer\b").unwrap(),
            "Note: will shut down the computer",
        ),
        (
            Regex::new(r"(?i)\brestart-computer\b").unwrap(),
            "Note: will restart the computer",
        ),
        (
            Regex::new(r"(?i)\bgit\s+reset\s+--hard\b").unwrap(),
            "Note: may discard uncommitted changes",
        ),
        (
            Regex::new(r"(?i)\bgit\s+push\b.*(--force|--force-with-lease|-f)\b").unwrap(),
            "Note: may overwrite remote history",
        ),
    ]
});

#[cfg(test)]
static POWERSHELL_TEST_OVERRIDE: LazyLock<Mutex<Option<PathBuf>>> =
    LazyLock::new(|| Mutex::new(None));
#[cfg(test)]
static POWERSHELL_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub struct PowerShellTool;

#[async_trait]
impl Tool for PowerShellTool {
    fn name(&self) -> &str {
        "powershell"
    }

    fn user_facing_name(&self) -> &str {
        "PowerShell"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["pwsh".to_string()]
    }

    fn activity_description(&self, params: &Value) -> String {
        let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("");
        format!("Running PowerShell: {}", command)
    }

    fn description(&self) -> &str {
        r#"Executes a PowerShell command and returns its output.

Use this when working in Windows/PowerShell-heavy environments. Prefer dedicated tools for reads/search/edits when possible:
- File search: use `glob`
- Content search: use `grep`
- File reads: use `read_file`
- File edits: use `edit_file` or `write_file`

This tool supports `run_in_background` and `timeout_ms` like the bash tool."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The PowerShell command to execute"
                },
                "description": {
                    "type": "string",
                    "description": "Short description of the command"
                },
                "run_in_background": {
                    "type": "boolean",
                    "default": false,
                    "description": "Whether to run the command in the background"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": format!("Optional timeout in milliseconds (max {}ms). Default: {}ms.", MAX_TIMEOUT_SECS * 1000, DEFAULT_TIMEOUT_SECS * 1000)
                }
            },
            "required": ["command"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: command"))?;

        let working_dir = ctx.working_dir.as_deref().unwrap_or_else(|| Path::new("."));
        let timeout_ms = params
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .or_else(|| params.get("timeout").and_then(|v| v.as_u64()));
        let timeout_secs = match timeout_ms {
            Some(t) if t >= 1000 => (t / 1000).min(MAX_TIMEOUT_SECS),
            Some(t) => t.min(MAX_TIMEOUT_SECS),
            None => DEFAULT_TIMEOUT_SECS,
        };
        let run_in_background = params
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let executable = match resolve_powershell_executable() {
            Ok(executable) => executable,
            Err(err) => return Ok(ToolResult::error(err.to_string())),
        };

        if run_in_background {
            return execute_background(&executable, command, working_dir, ctx).await;
        }

        let child = Command::new(&executable)
            .arg("-NoProfile")
            .arg("-Command")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(working_dir)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn PowerShell: {}", e))?;

        let output = match tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            child.wait_with_output(),
        )
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Ok(ToolResult::error(format!(
                    "Failed to execute PowerShell command: {}",
                    e
                )))
            }
            Err(_) => {
                return Ok(ToolResult::error_typed(
                    format!("Command timed out after {} seconds", timeout_secs),
                    ToolErrorType::Timeout,
                    true,
                    Some("Increase timeout or reduce scope.".to_string()),
                ));
            }
        };

        format_output(&executable, command, working_dir, output)
    }
}

fn format_output(
    executable: &Path,
    command: &str,
    working_dir: &Path,
    output: std::process::Output,
) -> Result<ToolResult> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let exit_code = output.status.code().unwrap_or(-1);

    let mut combined = String::new();
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str("[stderr]\n");
        combined.push_str(&stderr);
    }

    if !output.status.success() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&format!("[exit code: {}]", exit_code));
        return Ok(ToolResult::error(combined));
    }

        let analysis = analyze_powershell_command(command);
        let mut metadata = json!({
            "command": command,
            "cwd": working_dir.display().to_string(),
            "shell": executable.display().to_string(),
            "command_type": analysis.command_type,
            "read_only": analysis.read_only,
        });
    if let Some(reason) = &analysis.read_only_reason {
        metadata["read_only_reason"] = json!(reason);
    }

    if let Some(warning) = analysis.destructive_warning {
        metadata["destructive_warning"] = json!(warning);
    }

    let suggestion = analysis.suggestion;
    if let Some(rewrite) = suggestion.as_deref() {
        metadata["rewrite_suggestion"] = json!(rewrite);
    }
    Ok(ToolResult {
        content: combined,
        is_error: false,
        error_type: None,
        recoverable: false,
        suggestion,
        metadata: Some(metadata),
    })
}

async fn execute_background(
    executable: &Path,
    command: &str,
    working_dir: &Path,
    ctx: &ToolContext,
) -> Result<ToolResult> {
    let Some(runtime_tasks) = &ctx.runtime_tasks else {
        let _child = Command::new(executable)
            .arg("-NoProfile")
            .arg("-Command")
            .arg(command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        return Ok(ToolResult::success(format!(
            "PowerShell command started in background: {}",
            command
        )));
    };

    let tasks_dir = working_dir.join(".yode").join("tasks");
    tokio::fs::create_dir_all(&tasks_dir).await?;
    let output_path = tasks_dir.join(format!("powershell-{}.log", Uuid::new_v4()));
    let output_path_str = output_path.display().to_string();
    let transcript_path = crate::runtime_tasks::latest_transcript_artifact_path(working_dir);
    let description = format!(
        "Background powershell: {}",
        command.chars().take(60).collect::<String>()
    );
    let (task, mut cancel_rx) = {
        let mut store = runtime_tasks.lock().await;
        store.create_with_transcript(
            "powershell".to_string(),
            "powershell".to_string(),
            description,
            output_path_str.clone(),
            transcript_path.clone(),
        )
    };

    let task_id = task.id.clone();
    let runtime_tasks = runtime_tasks.clone();
    let working_dir = working_dir.to_path_buf();
    let command = command.to_string();
    let launch_command = command.clone();
    let executable = executable.to_path_buf();
    let output_path_spawn = output_path.clone();
    tokio::spawn(async move {
        {
            let mut store = runtime_tasks.lock().await;
            store.mark_running(&task_id);
            store.update_progress(&task_id, format!("Running {}", command));
        }

        let stdout_file = match std::fs::File::create(&output_path_spawn) {
            Ok(file) => file,
            Err(err) => {
                runtime_tasks
                    .lock()
                    .await
                    .mark_failed(&task_id, format!("Failed to create output file: {}", err));
                return;
            }
        };
        let stderr_file = match stdout_file.try_clone() {
            Ok(file) => file,
            Err(err) => {
                runtime_tasks.lock().await.mark_failed(
                    &task_id,
                    format!("Failed to clone output file handle: {}", err),
                );
                return;
            }
        };

        let mut child = match Command::new(&executable)
            .arg("-NoProfile")
            .arg("-Command")
            .arg(&command)
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file))
            .current_dir(&working_dir)
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                runtime_tasks.lock().await.mark_failed(
                    &task_id,
                    format!("Failed to spawn background command: {}", err),
                );
                return;
            }
        };

        let (done_tx, mut done_rx) = tokio::sync::watch::channel(false);
        let runtime_tasks_monitor = runtime_tasks.clone();
        let task_id_monitor = task_id.clone();
        let output_path_monitor = output_path_spawn.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3));
            let mut last_preview = String::new();
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Ok(content) = tokio::fs::read_to_string(&output_path_monitor).await {
                            if let Some(line) = content.lines().rev().find(|line| !line.trim().is_empty()) {
                                let preview = if line.chars().count() > 120 {
                                    let shortened = line.chars().take(120).collect::<String>();
                                    format!("{}...", shortened)
                                } else {
                                    line.to_string()
                                };
                                if preview != last_preview {
                                    runtime_tasks_monitor
                                        .lock()
                                        .await
                                        .update_progress(&task_id_monitor, preview.clone());
                                    last_preview = preview;
                                }
                            }
                        }
                    }
                    changed = done_rx.changed() => {
                        if changed.is_ok() && *done_rx.borrow() {
                            break;
                        }
                    }
                }
            }
        });

        tokio::select! {
            wait_result = child.wait() => {
                let _ = done_tx.send(true);
                match wait_result {
                    Ok(status) if status.success() => {
                        runtime_tasks.lock().await.mark_completed(&task_id);
                    }
                    Ok(status) => {
                        runtime_tasks.lock().await.mark_failed(
                            &task_id,
                            format!("Background command exited with status {}", status),
                        );
                    }
                    Err(err) => {
                        runtime_tasks
                            .lock()
                            .await
                            .mark_failed(&task_id, format!("Failed to wait for command: {}", err));
                    }
                }
            }
            changed = cancel_rx.changed() => {
                if changed.is_ok() && *cancel_rx.borrow() {
                    let _ = child.kill().await;
                    let _ = done_tx.send(true);
                    runtime_tasks.lock().await.mark_cancelled(&task_id);
                }
            }
        }
    });

    Ok(ToolResult::success_with_metadata(
        format!("Background PowerShell task started: {} ({})", task.id, launch_command),
        json!({
            "task_id": task.id,
            "task_kind": "powershell",
            "output_path": output_path_str,
            "transcript_path": transcript_path,
            "run_in_background": true,
        }),
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PowerShellAnalysis {
    command_type: &'static str,
    read_only: bool,
    read_only_reason: Option<String>,
    destructive_warning: Option<&'static str>,
    suggestion: Option<String>,
}

fn analyze_powershell_command(command: &str) -> PowerShellAnalysis {
    let command_type = classify_powershell_command(command);
    let read_only_validation = validate_read_only_powershell_command(command);
    let read_only = read_only_validation.is_safe;
    let destructive_warning = get_destructive_command_warning(command);
    let suggestion = suggest_safe_rewrite(command, command_type);
    PowerShellAnalysis {
        command_type,
        read_only,
        read_only_reason: read_only_validation.reason,
        destructive_warning,
        suggestion,
    }
}

fn classify_powershell_command(command: &str) -> &'static str {
    let segments = split_powershell_segments(command);
    let commands = segments
        .iter()
        .filter_map(|segment| first_segment_command(segment))
        .collect::<Vec<_>>();

    if commands
        .iter()
        .any(|cmd| PS_SEARCH_COMMANDS.iter().any(|candidate| *candidate == cmd))
    {
        "search"
    } else if commands.iter().any(|cmd| {
        PS_READ_COMMANDS.iter().any(|candidate| *candidate == cmd)
            || PS_READONLY_NAVIGATION_COMMANDS
                .iter()
                .any(|candidate| *candidate == cmd)
    }) {
        "read"
    } else {
        "generic"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReadOnlyValidation {
    is_safe: bool,
    reason: Option<String>,
}

fn validate_read_only_powershell_command(command: &str) -> ReadOnlyValidation {
    let segments = split_powershell_segments(command);
    if segments.is_empty() {
        return ReadOnlyValidation {
            is_safe: false,
            reason: Some("empty command".to_string()),
        };
    }

    for segment in segments {
        let tokens = tokenize_powershell_segment(&segment);
        let Some(cmd) = tokens.first() else {
            continue;
        };
        let Some(config) = read_only_config(cmd) else {
            return ReadOnlyValidation {
                is_safe: false,
                reason: Some(format!("non-read-only command: {}", cmd)),
            };
        };

        if tokens.iter().skip(1).any(|token| looks_like_redirection(token)) {
            return ReadOnlyValidation {
                is_safe: false,
                reason: Some(format!("redirection detected in {}", cmd)),
            };
        }

        if !config.allow_all_flags {
            for token in tokens.iter().skip(1) {
                if is_flag_token(token)
                    && !config
                        .safe_flags
                        .iter()
                        .any(|flag| flag.eq_ignore_ascii_case(token))
                {
                    return ReadOnlyValidation {
                        is_safe: false,
                        reason: Some(format!("unsafe flag {} for {}", token, cmd)),
                    };
                }
            }
        }

        if cmd == "git" && !validate_git_read_only_tokens(&tokens) {
            return ReadOnlyValidation {
                is_safe: false,
                reason: Some("non-read-only git subcommand".to_string()),
            };
        }
    }

    ReadOnlyValidation {
        is_safe: true,
        reason: Some("validated read-only command".to_string()),
    }
}

fn get_destructive_command_warning(command: &str) -> Option<&'static str> {
    DESTRUCTIVE_PATTERNS
        .iter()
        .find_map(|entry| {
            let (pattern, warning) = entry;
            pattern.is_match(command).then_some(*warning)
        })
}

fn suggest_safe_rewrite(command: &str, command_type: &str) -> Option<String> {
    match command_type {
        "search" => Some(
            "Prefer `grep` or `glob` tools for structured search results when PowerShell is not specifically required."
                .to_string(),
        ),
        "read" => Some(
            "Prefer `read_file` for file reads so the agent keeps precise file context.".to_string(),
        ),
        _ => {
            let first = first_segment_command(command).unwrap_or_default();
            if first == "set-location" || first == "push-location" || first == "pop-location" {
                Some(
                    "Prefer absolute paths and avoid shell-only cwd changes unless the user specifically wants a PowerShell workflow."
                        .to_string(),
                )
            } else {
                None
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ReadOnlyCommandConfig {
    safe_flags: &'static [&'static str],
    allow_all_flags: bool,
}

fn read_only_config(command: &str) -> Option<ReadOnlyCommandConfig> {
    let cmd = command.to_ascii_lowercase();
    match cmd.as_str() {
        "select-string" => Some(ReadOnlyCommandConfig {
            safe_flags: &[
                "-Path",
                "-Pattern",
                "-SimpleMatch",
                "-CaseSensitive",
                "-Quiet",
                "-List",
                "-NotMatch",
                "-AllMatches",
                "-Encoding",
                "-Context",
                "-Raw",
            ],
            allow_all_flags: false,
        }),
        "get-content" => Some(ReadOnlyCommandConfig {
            safe_flags: &[
                "-Path",
                "-LiteralPath",
                "-TotalCount",
                "-Head",
                "-Tail",
                "-Raw",
                "-Encoding",
                "-Delimiter",
                "-ReadCount",
            ],
            allow_all_flags: false,
        }),
        "get-item" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-Path", "-LiteralPath", "-Force", "-Stream"],
            allow_all_flags: false,
        }),
        "get-itemproperty" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-Path", "-LiteralPath", "-Name"],
            allow_all_flags: false,
        }),
        "test-path" => Some(ReadOnlyCommandConfig {
            safe_flags: &[
                "-Path",
                "-LiteralPath",
                "-PathType",
                "-Filter",
                "-Include",
                "-Exclude",
                "-IsValid",
            ],
            allow_all_flags: false,
        }),
        "resolve-path" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-Path", "-LiteralPath", "-Relative"],
            allow_all_flags: false,
        }),
        "get-filehash" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-Path", "-LiteralPath", "-Algorithm"],
            allow_all_flags: false,
        }),
        "get-acl" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-Path", "-LiteralPath", "-Audit"],
            allow_all_flags: false,
        }),
        "get-command" | "get-help" | "get-module" | "get-alias" => Some(ReadOnlyCommandConfig {
            safe_flags: &[],
            allow_all_flags: true,
        }),
        "get-process" | "get-service" | "get-location" | "format-hex" | "where.exe"
        | "findstr" => Some(ReadOnlyCommandConfig {
            safe_flags: &[],
            allow_all_flags: true,
        }),
        "get-childitem" => Some(ReadOnlyCommandConfig {
            safe_flags: &[
                "-Path",
                "-LiteralPath",
                "-Filter",
                "-Include",
                "-Exclude",
                "-Recurse",
                "-Depth",
                "-Name",
                "-Force",
                "-Directory",
                "-File",
            ],
            allow_all_flags: false,
        }),
        "set-location" | "push-location" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-Path", "-LiteralPath", "-PassThru", "-StackName"],
            allow_all_flags: false,
        }),
        "pop-location" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-PassThru", "-StackName"],
            allow_all_flags: false,
        }),
        "git" => Some(ReadOnlyCommandConfig {
            safe_flags: &[],
            allow_all_flags: true,
        }),
        _ => None,
    }
}

fn split_powershell_segments(command: &str) -> Vec<String> {
    command
        .split(|ch| matches!(ch, ';' | '\n' | '\r' | '|'))
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn tokenize_powershell_segment(segment: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for ch in segment.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn first_segment_command(segment: &str) -> Option<String> {
    tokenize_powershell_segment(segment)
        .into_iter()
        .next()
        .map(|token| token.to_ascii_lowercase())
}

fn is_flag_token(token: &str) -> bool {
    token.starts_with('-') && token.len() > 1
}

fn looks_like_redirection(token: &str) -> bool {
    token == ">" || token == ">>" || token == "2>" || token == "2>>"
}

fn validate_git_read_only_tokens(tokens: &[String]) -> bool {
    let Some(subcommand) = tokens.get(1) else {
        return false;
    };
    PS_GIT_READONLY_SUBCOMMANDS
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(subcommand))
}

fn resolve_powershell_executable() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(path) = POWERSHELL_TEST_OVERRIDE.lock().unwrap().clone() {
        return Ok(path);
    }

    if let Ok(path) = std::env::var("YODE_POWERSHELL_PATH") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
    }

    for candidate in ["pwsh", "powershell"] {
        if let Some(path) = find_in_path(candidate) {
            return Ok(path);
        }
    }

    Err(anyhow::anyhow!(
        "PowerShell executable not found. Install 'pwsh' or set YODE_POWERSHELL_PATH."
    ))
}

fn find_in_path(command: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let paths = std::env::split_paths(&path_var);

    #[cfg(windows)]
    let candidates = [format!("{}.exe", command), command.to_string()];
    #[cfg(not(windows))]
    let candidates = [command.to_string()];

    for dir in paths {
        for candidate in &candidates {
            let full = dir.join(candidate);
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}

#[cfg(test)]
fn set_powershell_test_override(path: Option<PathBuf>) {
    *POWERSHELL_TEST_OVERRIDE.lock().unwrap() = path;
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;
    use tokio::sync::Mutex;

    use crate::runtime_tasks::RuntimeTaskStore;
    use crate::tool::{Tool, ToolContext};

    use super::{
        analyze_powershell_command, classify_powershell_command,
        get_destructive_command_warning, set_powershell_test_override, suggest_safe_rewrite,
        PowerShellTool, POWERSHELL_TEST_LOCK,
    };

    fn write_shim(dir: &tempfile::TempDir) -> std::path::PathBuf {
        #[cfg(windows)]
        let path = dir.path().join("pwsh-shim.cmd");
        #[cfg(not(windows))]
        let path = dir.path().join("pwsh-shim");

        #[cfg(windows)]
        fs::write(
            &path,
            "@echo off\r\nsetlocal\r\nset \"cmd=\"\r\n:loop\r\nif \"%~1\"==\"\" goto end\r\nif /I \"%~1\"==\"-Command\" (\r\n  shift\r\n  set \"cmd=%~1\"\r\n  goto end\r\n)\r\nshift\r\ngoto loop\r\n:end\r\nif \"%cmd%\"==\"\" exit /b 2\r\npowershell -NoProfile -Command \"%cmd%\"\r\n",
        )
        .unwrap();
        #[cfg(not(windows))]
        fs::write(
            &path,
            "#!/bin/sh\ncmd=\"\"\nwhile [ $# -gt 0 ]; do\n  if [ \"$1\" = \"-Command\" ]; then\n    shift\n    cmd=\"$1\"\n    break\n  fi\n  shift\ndone\n[ -z \"$cmd\" ] && exit 2\nsh -c \"$cmd\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
        }
        path
    }

    #[tokio::test]
    async fn powershell_runs_simple_command_via_override() {
        let _guard = POWERSHELL_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim(&dir);
        set_powershell_test_override(Some(shim));

        let result = PowerShellTool
            .execute(json!({"command": "echo hello"}), &ToolContext::empty())
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("hello"));
        set_powershell_test_override(None);
    }

    #[tokio::test]
    async fn powershell_reports_non_zero_exit_code() {
        let _guard = POWERSHELL_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim(&dir);
        set_powershell_test_override(Some(shim));

        let result = PowerShellTool
            .execute(json!({"command": "exit 3"}), &ToolContext::empty())
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("exit code: 3"));
        set_powershell_test_override(None);
    }

    #[tokio::test]
    async fn powershell_timeout_is_reported() {
        let _guard = POWERSHELL_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim(&dir);
        set_powershell_test_override(Some(shim));

        let result = PowerShellTool
            .execute(
                json!({"command": "sleep 10", "timeout_ms": 1000}),
                &ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("timed out"));
        set_powershell_test_override(None);
    }

    #[tokio::test]
    async fn powershell_background_registers_runtime_task() {
        let _guard = POWERSHELL_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim(&dir);
        set_powershell_test_override(Some(shim));

        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.runtime_tasks = Some(Arc::new(Mutex::new(RuntimeTaskStore::new())));

        let result = PowerShellTool
            .execute(
                json!({"command": "echo hello", "run_in_background": true}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        let task_id = result.metadata.as_ref().unwrap()["task_id"]
            .as_str()
            .unwrap()
            .to_string();

        tokio::time::sleep(Duration::from_millis(100)).await;
        let tasks = ctx.runtime_tasks.as_ref().unwrap().lock().await.list();
        assert!(tasks.iter().any(|task| task.id == task_id));
        set_powershell_test_override(None);
    }

    #[test]
    fn powershell_classifies_read_search_and_destructive_commands() {
        assert_eq!(classify_powershell_command("Get-Content foo.txt"), "read");
        assert_eq!(classify_powershell_command("Select-String foo bar.txt"), "search");
        assert_eq!(classify_powershell_command("Write-Host hi"), "generic");
        assert_eq!(classify_powershell_command("Get-Help Get-Item"), "read");
        assert!(analyze_powershell_command("Get-Content foo.txt").read_only);
        assert!(analyze_powershell_command("Set-Location src").read_only);
        assert!(analyze_powershell_command("Get-Command cargo").read_only);
        assert!(!analyze_powershell_command("Remove-Item foo").read_only);
        assert!(get_destructive_command_warning("Remove-Item -Recurse -Force tmp")
            .unwrap()
            .contains("remove"));
        assert!(get_destructive_command_warning("Get-Content foo.txt").is_none());
        assert!(suggest_safe_rewrite("Get-Content foo.txt", "read")
            .unwrap()
            .contains("read_file"));
        assert!(suggest_safe_rewrite("Select-String foo", "search")
            .unwrap()
            .contains("grep"));
    }

    #[test]
    fn powershell_analysis_combines_semantics() {
        let analysis = analyze_powershell_command("Get-Command cargo");
        assert_eq!(analysis.command_type, "read");
        assert!(analysis.read_only);
        assert_eq!(
            analysis.read_only_reason.as_deref(),
            Some("validated read-only command")
        );
        assert!(analysis.destructive_warning.is_none());
        assert!(analysis.suggestion.unwrap().contains("read_file"));

        let destructive = analyze_powershell_command("Remove-Item -Recurse -Force tmp");
        assert_eq!(destructive.command_type, "generic");
        assert!(!destructive.read_only);
        assert!(destructive.destructive_warning.is_some());
    }

    #[test]
    fn powershell_analysis_rejects_unsafe_flags_redirection_and_git_writes() {
        let unsafe_flag = analyze_powershell_command("Get-Content -Wait foo.txt");
        assert!(!unsafe_flag.read_only);
        assert!(unsafe_flag
            .read_only_reason
            .as_deref()
            .unwrap_or("")
            .contains("unsafe flag"));

        let redirected = analyze_powershell_command("Get-Content foo.txt > out.txt");
        assert!(!redirected.read_only);
        assert!(redirected
            .read_only_reason
            .as_deref()
            .unwrap_or("")
            .contains("redirection"));

        let git_write = analyze_powershell_command("git commit -m test");
        assert!(!git_write.read_only);
        assert!(git_write
            .read_only_reason
            .as_deref()
            .unwrap_or("")
            .contains("git"));
    }

    #[test]
    fn powershell_dangerous_command_detection_is_broad() {
        assert!(get_destructive_command_warning("Remove-Item -Recurse -Force tmp").is_some());
        assert!(get_destructive_command_warning("git push --force").is_some());
        assert!(analyze_powershell_command("Remove-Item -Recurse -Force tmp")
            .destructive_warning
            .is_some());
        assert!(analyze_powershell_command("git push --force")
            .destructive_warning
            .is_some());
        assert!(analyze_powershell_command("Get-Content foo.txt")
            .destructive_warning
            .is_none());
    }
}
