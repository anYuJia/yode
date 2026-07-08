use std::collections::HashMap;
use std::path::PathBuf;

use tauri::{AppHandle, Emitter};

use crate::protocol::{TerminalExitEvent, TerminalOutputEvent};

pub(super) fn emit_terminal_output(app: &AppHandle, event: TerminalOutputEvent) {
    let session_id = event.session_id.clone();
    if let Err(err) = app.emit("terminal-output", event) {
        tracing::warn!(
            session_id = %session_id,
            error = %err,
            "Failed to emit terminal output event"
        );
    }
}

pub(super) fn emit_terminal_exit(app: &AppHandle, event: TerminalExitEvent) {
    let session_id = event.session_id.clone();
    if let Err(err) = app.emit("terminal-exit", event) {
        tracing::warn!(
            session_id = %session_id,
            error = %err,
            "Failed to emit terminal exit event"
        );
    }
}

pub(super) fn valid_terminal_cwd(raw: &str) -> Option<PathBuf> {
    let path = PathBuf::from(raw);
    if path.is_dir() {
        Some(path)
    } else {
        None
    }
}

pub(super) fn terminal_shell_command(
    env: &HashMap<String, String>,
) -> (PathBuf, Vec<&'static str>) {
    let shell = env
        .get("SHELL")
        .filter(|shell| !shell.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/bin/sh"));
    let shell_name = shell
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if shell_name.contains("zsh") || shell_name.contains("bash") {
        (shell, vec!["-lic"])
    } else {
        (PathBuf::from("/bin/sh"), vec!["-lc"])
    }
}

pub(super) fn apply_terminal_color_env(command: &mut portable_pty::CommandBuilder) {
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("CLICOLOR", "1");
    command.env("FORCE_COLOR", "1");
    command.env("GREP_COLORS", "mt=01;35:fn=36:ln=32:se=2");
}

pub(super) fn parse_terminal_run_stdout(
    stdout: &[u8],
    marker: &str,
    fallback_cwd: &std::path::Path,
    fallback_env: &HashMap<String, String>,
    fallback_exit_code: i32,
) -> (String, PathBuf, HashMap<String, String>, i32) {
    let status_marker = format!("\n{}STATUS:", marker).into_bytes();
    let Some(status_start) = find_bytes(stdout, &status_marker) else {
        return (
            String::from_utf8_lossy(stdout).trim_end().to_string(),
            fallback_cwd.to_path_buf(),
            fallback_env.clone(),
            fallback_exit_code,
        );
    };

    let visible_stdout = String::from_utf8_lossy(&stdout[..status_start])
        .trim_end_matches('\n')
        .to_string();
    let status_value_start = status_start + status_marker.len();
    let status_end = stdout[status_value_start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|offset| status_value_start + offset)
        .unwrap_or(stdout.len());
    let exit_code = String::from_utf8_lossy(&stdout[status_value_start..status_end])
        .trim()
        .parse::<i32>()
        .unwrap_or(fallback_exit_code);

    let pwd_marker = format!("{}PWD:", marker).into_bytes();
    let env_marker = format!("{}ENV:", marker).into_bytes();
    let pwd_start =
        find_bytes_from(stdout, &pwd_marker, status_end).map(|idx| idx + pwd_marker.len());
    let env_start = find_bytes_from(stdout, &env_marker, status_end);

    let cwd = pwd_start
        .and_then(|start| {
            let end = stdout[start..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|offset| start + offset)
                .unwrap_or(stdout.len());
            let path = String::from_utf8_lossy(&stdout[start..end])
                .trim()
                .to_string();
            if path.is_empty() {
                None
            } else {
                Some(PathBuf::from(path))
            }
        })
        .unwrap_or_else(|| fallback_cwd.to_path_buf());

    let env = env_start
        .map(|start| parse_null_delimited_env(&stdout[start + env_marker.len()..]))
        .filter(|env| !env.is_empty())
        .unwrap_or_else(|| fallback_env.clone());

    (visible_stdout, cwd, env, exit_code)
}

fn parse_null_delimited_env(bytes: &[u8]) -> HashMap<String, String> {
    bytes
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            if entry.is_empty() {
                return None;
            }
            let eq = entry.iter().position(|byte| *byte == b'=')?;
            let key = String::from_utf8_lossy(&entry[..eq]).to_string();
            let value = String::from_utf8_lossy(&entry[eq + 1..]).to_string();
            Some((key, value))
        })
        .collect()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes_from(haystack, needle, 0)
}

fn find_bytes_from(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() || start >= haystack.len() {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}
