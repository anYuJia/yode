use std::env;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Command as StdCommand;
#[cfg(target_os = "linux")]
use std::sync::OnceLock;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::tool::ToolResult;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxMode {
    Off,
    Auto,
    Strict,
}

impl SandboxMode {
    pub fn from_env() -> Self {
        match env::var("YODE_SANDBOX_MODE")
            .unwrap_or_else(|_| "auto".to_string())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "off" | "disabled" | "none" => Self::Off,
            "strict" | "required" => Self::Strict,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxNetworkPolicy {
    Inherit,
    Deny,
}

impl SandboxNetworkPolicy {
    fn from_env() -> Self {
        match env::var("YODE_SANDBOX_NETWORK")
            .unwrap_or_else(|_| "inherit".to_string())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "deny" | "off" | "none" => Self::Deny,
            _ => Self::Inherit,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxBackend {
    Bubblewrap,
    SandboxExec,
    Unavailable,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxLaunchInfo {
    pub mode: SandboxMode,
    pub backend: SandboxBackend,
    pub sandboxed: bool,
    pub network: SandboxNetworkPolicy,
    pub degraded_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreparedShell {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub info: SandboxLaunchInfo,
}

/// Prepare a shell command behind a real OS sandbox when one is available.
///
/// Linux uses bubblewrap with a read-only host filesystem and a writable workspace.
/// macOS uses sandbox-exec with writes scoped to the workspace and temporary directory.
/// Windows currently has no dependency-free native restricted-token backend in Yode; Strict
/// therefore fails closed instead of pretending the command is isolated.
pub fn prepare_shell(
    command: &str,
    working_dir: &Path,
    explicitly_disabled: bool,
) -> Result<PreparedShell> {
    let mode = if explicitly_disabled {
        SandboxMode::Off
    } else {
        SandboxMode::from_env()
    };
    let network = SandboxNetworkPolicy::from_env();
    #[cfg(target_os = "windows")]
    let _ = working_dir;

    if mode == SandboxMode::Off {
        let (executable, args) = plain_shell(command);
        return Ok(PreparedShell {
            executable,
            args,
            info: SandboxLaunchInfo {
                mode,
                backend: SandboxBackend::Disabled,
                sandboxed: false,
                network,
                degraded_reason: explicitly_disabled
                    .then(|| "sandbox explicitly disabled for this command".to_string()),
            },
        });
    }

    #[cfg(target_os = "linux")]
    let linux_bwrap_error = match usable_bwrap() {
        Ok(bwrap) => {
            let cwd = working_dir
                .canonicalize()
                .unwrap_or_else(|_| working_dir.to_path_buf());
            let mut args = vec![
                "--die-with-parent".to_string(),
                "--new-session".to_string(),
                "--ro-bind".to_string(),
                "/".to_string(),
                "/".to_string(),
                "--bind".to_string(),
                cwd.display().to_string(),
                cwd.display().to_string(),
                "--chdir".to_string(),
                cwd.display().to_string(),
                "--proc".to_string(),
                "/proc".to_string(),
                "--dev".to_string(),
                "/dev".to_string(),
                "--tmpfs".to_string(),
                "/tmp".to_string(),
            ];
            if network == SandboxNetworkPolicy::Deny {
                args.push("--unshare-net".to_string());
            }
            args.extend([
                "--".to_string(),
                "sh".to_string(),
                "-c".to_string(),
                command.to_string(),
            ]);
            return Ok(PreparedShell {
                executable: bwrap,
                args,
                info: SandboxLaunchInfo {
                    mode,
                    backend: SandboxBackend::Bubblewrap,
                    sandboxed: true,
                    network,
                    degraded_reason: None,
                },
            });
        }
        Err(reason) => reason,
    };

    #[cfg(target_os = "macos")]
    if let Some(sandbox_exec) = find_executable("sandbox-exec") {
        let cwd = working_dir
            .canonicalize()
            .unwrap_or_else(|_| working_dir.to_path_buf());
        let cwd = escape_sandbox_literal(&cwd.display().to_string());
        let mut profile = format!(
            "(version 1)(deny default)(allow process*)(allow sysctl-read)(allow file-read*)(allow file-write* (subpath \"{cwd}\"))(allow file-write* (subpath \"/tmp\"))(allow file-write* (subpath \"/private/tmp\"))"
        );
        if network == SandboxNetworkPolicy::Inherit {
            profile.push_str("(allow network*)");
        }
        return Ok(PreparedShell {
            executable: sandbox_exec,
            args: vec![
                "-p".to_string(),
                profile,
                "sh".to_string(),
                "-c".to_string(),
                command.to_string(),
            ],
            info: SandboxLaunchInfo {
                mode,
                backend: SandboxBackend::SandboxExec,
                sandboxed: true,
                network,
                degraded_reason: None,
            },
        });
    }

    #[cfg(target_os = "linux")]
    let reason = linux_bwrap_error;
    #[cfg(not(target_os = "linux"))]
    let reason = platform_unavailable_reason();
    if mode == SandboxMode::Strict {
        bail!("OS sandbox required but unavailable: {reason}");
    }

    let (executable, args) = plain_shell(command);
    Ok(PreparedShell {
        executable,
        args,
        info: SandboxLaunchInfo {
            mode,
            backend: SandboxBackend::Unavailable,
            sandboxed: false,
            network,
            degraded_reason: Some(reason),
        },
    })
}

pub fn annotate_tool_result(result: &mut ToolResult, info: &SandboxLaunchInfo) {
    let metadata = result.metadata.get_or_insert_with(|| json!({}));
    if !metadata.is_object() {
        *metadata = json!({});
    }
    if let Some(object) = metadata.as_object_mut() {
        object.insert(
            "sandbox".to_string(),
            serde_json::to_value(info).unwrap_or_else(|_| json!({})),
        );
    }
}

fn plain_shell(command: &str) -> (PathBuf, Vec<String>) {
    #[cfg(target_os = "windows")]
    {
        return (
            PathBuf::from("cmd.exe"),
            vec![
                "/d".to_string(),
                "/s".to_string(),
                "/c".to_string(),
                command.to_string(),
            ],
        );
    }
    #[cfg(not(target_os = "windows"))]
    {
        (
            PathBuf::from("sh"),
            vec!["-c".to_string(), command.to_string()],
        )
    }
}

#[cfg(target_os = "linux")]
static BWRAP_PROBE: OnceLock<Result<PathBuf, String>> = OnceLock::new();

#[cfg(target_os = "linux")]
fn usable_bwrap() -> Result<PathBuf, String> {
    BWRAP_PROBE
        .get_or_init(|| resolve_bwrap_candidate(find_executable("bwrap"), probe_bwrap))
        .clone()
}

#[cfg(target_os = "linux")]
fn resolve_bwrap_candidate<F>(candidate: Option<PathBuf>, probe: F) -> Result<PathBuf, String>
where
    F: FnOnce(&Path) -> Result<(), String>,
{
    let bwrap = candidate
        .ok_or_else(|| "bubblewrap (bwrap) is not installed or not on PATH".to_string())?;
    probe(&bwrap)?;
    Ok(bwrap)
}

#[cfg(target_os = "linux")]
fn probe_bwrap(bwrap: &Path) -> Result<(), String> {
    let output = StdCommand::new(bwrap)
        .args([
            "--die-with-parent",
            "--new-session",
            "--ro-bind",
            "/",
            "/",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--",
            "sh",
            "-c",
            "true",
        ])
        .output()
        .map_err(|error| format!("bubblewrap probe could not start: {error}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(240).collect::<String>())
        .unwrap_or_else(|| format!("probe exited with {}", output.status));
    Err(format!("bubblewrap is installed but unusable: {detail}"))
}

fn platform_unavailable_reason() -> String {
    #[cfg(target_os = "linux")]
    {
        return "bubblewrap (bwrap) is not installed or not on PATH".to_string();
    }
    #[cfg(target_os = "macos")]
    {
        return "sandbox-exec is unavailable on this macOS installation".to_string();
    }
    #[cfg(target_os = "windows")]
    {
        return "native restricted-token sandbox backend is not available; strict mode refuses unsandboxed execution".to_string();
    }
    #[allow(unreachable_code)]
    "no supported OS sandbox backend is available".to_string()
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(name);
    if direct.components().count() > 1 && direct.is_file() {
        return Some(direct);
    }
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(target_os = "macos")]
fn escape_sandbox_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_mode_uses_platform_shell() {
        let prepared = prepare_shell("echo ok", Path::new("."), true).unwrap();
        assert_eq!(prepared.info.backend, SandboxBackend::Disabled);
        assert!(!prepared.info.sandboxed);
        #[cfg(target_os = "windows")]
        assert_eq!(prepared.executable, PathBuf::from("cmd.exe"));
        #[cfg(not(target_os = "windows"))]
        assert_eq!(prepared.executable, PathBuf::from("sh"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unusable_bwrap_candidate_is_rejected_before_command_execution() {
        let candidate = PathBuf::from("/usr/bin/bwrap");
        let error = resolve_bwrap_candidate(Some(candidate), |_| {
            Err("bubblewrap is installed but unusable: uid map denied".to_string())
        })
        .unwrap_err();
        assert!(error.contains("unusable"));
        assert!(error.contains("uid map denied"));
    }

    #[test]
    fn annotation_preserves_result_metadata() {
        let mut result =
            ToolResult::success_with_metadata("ok".to_string(), json!({"existing": true}));
        annotate_tool_result(
            &mut result,
            &SandboxLaunchInfo {
                mode: SandboxMode::Auto,
                backend: SandboxBackend::Unavailable,
                sandboxed: false,
                network: SandboxNetworkPolicy::Inherit,
                degraded_reason: Some("test".to_string()),
            },
        );
        assert_eq!(
            result
                .metadata
                .as_ref()
                .and_then(|m| m.get("existing"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(result
            .metadata
            .as_ref()
            .and_then(|m| m.get("sandbox"))
            .is_some());
    }
}
