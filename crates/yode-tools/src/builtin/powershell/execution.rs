use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use serde_json::json;
use tokio::process::Command;

use crate::builtin::shell_runtime::{
    command_timeout_secs, execute_background_shell, BackgroundShellSpec,
};
use crate::tool::{ToolContext, ToolErrorType, ToolResult};

use super::analysis::{analyze_powershell_command, get_destructive_command_warning};

pub(super) async fn execute_powershell_command(
    command: &str,
    params: &serde_json::Value,
    working_dir: &Path,
    ctx: &ToolContext,
) -> Result<ToolResult> {
    let timeout_secs = command_timeout_secs(params);
    let run_in_background = params
        .get("run_in_background")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    if let Some(warning) = get_destructive_command_warning(command) {
        return Ok(ToolResult::error_typed(
            format!(
                "Refusing to run potentially destructive PowerShell command: {}\nCommand: {}",
                warning, command
            ),
            ToolErrorType::Permission,
            false,
            Some(
                "Use a narrower, reversible command or ask the user for an explicit manual recovery action."
                    .to_string(),
            ),
        ));
    }

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
        .map_err(|err| anyhow::anyhow!("Failed to spawn PowerShell: {}", err))?;

    let output =
        match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output())
            .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(err)) => {
                return Ok(ToolResult::error(format!(
                    "Failed to execute PowerShell command: {}",
                    err
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
    execute_background_shell(
        BackgroundShellSpec {
            executable,
            args: vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                command.to_string(),
            ],
            command_display: command,
            task_kind: "powershell",
            description_prefix: "Background powershell",
            start_message: "PowerShell command started in background",
        },
        working_dir,
        ctx,
    )
    .await
}

fn resolve_powershell_executable() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(path) = super::POWERSHELL_TEST_OVERRIDE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
    {
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
