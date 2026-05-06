mod analysis;
mod execution;

use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use std::sync::{LazyLock, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::builtin::shell_runtime::timeout_ms_description;
use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};
use execution::execute_powershell_command;

#[cfg(test)]
static POWERSHELL_TEST_OVERRIDE: LazyLock<Mutex<Option<PathBuf>>> =
    LazyLock::new(|| Mutex::new(None));
#[cfg(test)]
static POWERSHELL_TEST_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

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
                    "description": timeout_ms_description()
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
        execute_powershell_command(command, &params, working_dir, ctx).await
    }
}

#[cfg(test)]
fn set_powershell_test_override(path: Option<PathBuf>) {
    *POWERSHELL_TEST_OVERRIDE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = path;
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

    use super::analysis::{
        analyze_powershell_command, classify_powershell_command, get_destructive_command_warning,
        suggest_safe_rewrite,
    };
    use super::{set_powershell_test_override, PowerShellTool, POWERSHELL_TEST_LOCK};

    struct PowerShellOverrideReset;

    impl Drop for PowerShellOverrideReset {
        fn drop(&mut self) {
            set_powershell_test_override(None);
        }
    }

    fn install_powershell_test_override(path: std::path::PathBuf) -> PowerShellOverrideReset {
        set_powershell_test_override(Some(path));
        PowerShellOverrideReset
    }

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
        let _guard = POWERSHELL_TEST_LOCK.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim(&dir);
        let _reset = install_powershell_test_override(shim);

        let result = PowerShellTool
            .execute(json!({"command": "echo hello"}), &ToolContext::empty())
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("hello"));
    }

    #[tokio::test]
    async fn powershell_reports_non_zero_exit_code() {
        let _guard = POWERSHELL_TEST_LOCK.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim(&dir);
        let _reset = install_powershell_test_override(shim);

        let result = PowerShellTool
            .execute(json!({"command": "exit 3"}), &ToolContext::empty())
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("exit code: 3"));
    }

    #[tokio::test]
    async fn powershell_timeout_is_reported() {
        let _guard = POWERSHELL_TEST_LOCK.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim(&dir);
        let _reset = install_powershell_test_override(shim);

        let result = PowerShellTool
            .execute(
                json!({"command": "sleep 10", "timeout_ms": 1000}),
                &ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("timed out"));
    }

    #[tokio::test]
    async fn powershell_background_registers_runtime_task() {
        let _guard = POWERSHELL_TEST_LOCK.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim(&dir);
        let _reset = install_powershell_test_override(shim);

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
    }

    #[test]
    fn powershell_classifies_read_search_and_destructive_commands() {
        assert_eq!(classify_powershell_command("Get-Content foo.txt"), "read");
        assert_eq!(
            classify_powershell_command("Select-String foo bar.txt"),
            "search"
        );
        assert_eq!(classify_powershell_command("Write-Host hi"), "generic");
        assert_eq!(classify_powershell_command("Get-Help Get-Item"), "read");
        assert!(analyze_powershell_command("Get-Content foo.txt").read_only);
        assert!(analyze_powershell_command("Set-Location src").read_only);
        assert!(analyze_powershell_command("Get-Command cargo").read_only);
        assert!(!analyze_powershell_command("Remove-Item foo").read_only);
        assert!(
            get_destructive_command_warning("Remove-Item -Recurse -Force tmp")
                .unwrap()
                .contains("remove")
        );
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
        assert!(
            analyze_powershell_command("Remove-Item -Recurse -Force tmp")
                .destructive_warning
                .is_some()
        );
        assert!(analyze_powershell_command("git push --force")
            .destructive_warning
            .is_some());
        assert!(analyze_powershell_command("Get-Content foo.txt")
            .destructive_warning
            .is_none());
    }
}
