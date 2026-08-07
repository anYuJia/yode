use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use std::path::{Path, PathBuf};

use yode_core::config::Config;

use super::{settings_system::open_with_destination, DesktopRuntime};
use crate::protocol::{
    ConfigurationState, ConfigurationUpdateRequest, DiagnosticCheck, WorkspaceDiagnosticsResult,
};

impl DesktopRuntime {
    pub async fn configuration_state(&self) -> Result<ConfigurationState> {
        let project_config_path = self.project_config_path();
        let mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?
            .as_str()
            .parse::<yode_core::permission::PermissionMode>()
            .unwrap_or(yode_core::permission::PermissionMode::Default);
        Ok(ConfigurationState {
            scope: if project_config_path.exists() {
                "Project config".to_string()
            } else {
                "User config".to_string()
            },
            approval_policy: approval_policy_from_permission_mode(mode),
            sandbox_settings: sandbox_settings_from_permission_mode(mode),
            expose_dependencies: load_workspace_dependency_state_async().await,
            config_path: self.user_config_path().display().to_string(),
            project_config_path: project_config_path.display().to_string(),
            effective_permission_mode: mode.to_string(),
        })
    }

    pub async fn configuration_update(
        &self,
        request: ConfigurationUpdateRequest,
    ) -> Result<ConfigurationState> {
        // 配置页不再维护第二套权限真相。审批与沙箱字段只作为兼容输入，
        // 返回值始终由后端当前有效模式重新推导；项目作用域尤其不能提权。
        let effective_mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?
            .parse::<yode_core::permission::PermissionMode>()
            .unwrap_or(yode_core::permission::PermissionMode::Default);
        // 若旧前端仍提交与有效模式冲突的权限字段，记录漂移以暴露真相源不一致。
        let expected = approval_policy_from_permission_mode(effective_mode);
        if request.approval_policy != expected {
            tracing::warn!(
                requested = %request.approval_policy,
                effective = %expected,
                "Ignoring stale approval_policy from configuration page; effective permission mode is authoritative."
            );
        }
        let expected_sandbox = sandbox_settings_from_permission_mode(effective_mode);
        if request.sandbox_settings != expected_sandbox {
            tracing::warn!(
                requested = %request.sandbox_settings,
                effective = %expected_sandbox,
                "Ignoring stale sandbox_settings from configuration page; effective permission mode is authoritative."
            );
        }
        set_workspace_dependency_state_async(request.expose_dependencies).await?;
        Ok(ConfigurationState {
            scope: request.scope,
            approval_policy: approval_policy_from_permission_mode(effective_mode),
            sandbox_settings: sandbox_settings_from_permission_mode(effective_mode),
            expose_dependencies: request.expose_dependencies,
            config_path: self.user_config_path().display().to_string(),
            project_config_path: self.project_config_path().display().to_string(),
            effective_permission_mode: effective_mode.to_string(),
        })
    }

    pub async fn open_configuration_file(&self, scope: String) -> Result<()> {
        let path = if scope.to_lowercase().contains("project") {
            self.project_config_path()
        } else {
            self.user_config_path()
        };
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if !tokio::fs::try_exists(&path).await? {
            let config = {
                self.config
                    .lock()
                    .map_err(|_| anyhow::anyhow!("config lock poisoned"))?
                    .clone()
            };
            if scope.to_lowercase().contains("project") {
                // 项目配置只允许包含可共享字段，绝不写入 API key / token
                save_project_config_to_path_async(&config, &path).await?;
            } else {
                save_config_to_path_async(&config, &path).await?;
            }
        }
        open_with_destination("VS Code", &path)
    }

    pub async fn diagnose_workspace(&self) -> Result<WorkspaceDiagnosticsResult> {
        let report_dir = self.workspace_path.join(".yode").join("diagnostics");
        tokio::fs::create_dir_all(&report_dir).await?;
        let report_path = report_dir.join(format!(
            "diagnostics-{}.md",
            Utc::now().format("%Y%m%d-%H%M%S")
        ));
        let checks = workspace_diagnostic_checks(self).await?;
        let mut report = String::from("# Yode 工作区诊断\n\n");
        for check in &checks {
            report.push_str(&format!(
                "- [{}] {}: {}\n",
                check.status, check.name, check.detail
            ));
        }
        tokio::fs::write(&report_path, report).await?;
        Ok(WorkspaceDiagnosticsResult {
            report_path: report_path.display().to_string(),
            checks,
        })
    }

    pub async fn reinstall_workspace(&self) -> Result<WorkspaceDiagnosticsResult> {
        let cache_dir = self.workspace_path.join(".yode").join("workspace");
        if tokio::fs::try_exists(&cache_dir).await? {
            tokio::fs::remove_dir_all(&cache_dir).await?;
        }
        tokio::fs::create_dir_all(&cache_dir).await?;
        tokio::fs::write(
            cache_dir.join("README.txt"),
            "Yode workspace dependencies are managed here.\n",
        )
        .await?;
        set_workspace_dependency_state_async(true).await?;
        self.diagnose_workspace().await
    }
}

pub(super) async fn load_desktop_config(workspace_path: &Path) -> Result<Config> {
    let user_config = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".yode")
        .join("config.toml");
    let _project_config = workspace_path.join(".yode").join("config.toml");
    // 未建立仓库外信任记录前，桌面端不执行任何仓库配置覆盖。尤其不能让
    // `.yode/config.toml` 改写 endpoint/API key、权限模式、MCP 或 Hooks。
    Config::load_with_overrides_async(Some(&user_config), None).await
}

pub(super) async fn save_config_to_path_async(config: &Config, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    atomic_write_async(path, toml::to_string_pretty(config)?.as_bytes()).await
}

/// 写项目级共享配置：只写脱敏后的可共享字段（不含 API key 与疑似密钥环境变量）。
/// 用户级配置仍然写入完整内容。
pub(super) async fn save_project_config_to_path_async(config: &Config, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut root = toml::map::Map::new();
    root.insert("ui".to_string(), toml::Value::try_from(&config.ui)?);
    if !config.permissions.always_deny.is_empty() {
        let mut permissions = toml::map::Map::new();
        permissions.insert(
            "always_deny".to_string(),
            toml::Value::try_from(&config.permissions.always_deny)?,
        );
        root.insert("permissions".to_string(), toml::Value::Table(permissions));
    }
    let serialized = toml::to_string_pretty(&toml::Value::Table(root))?;
    atomic_write_async(path, serialized.as_bytes()).await
}

async fn atomic_write_async(path: &Path, bytes: &[u8]) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    let temporary = path.with_file_name(format!(".{file_name}.tmp-{}", std::process::id()));
    let mut file = tokio::fs::File::create(&temporary).await?;
    file.write_all(bytes).await?;
    file.sync_all().await?;
    drop(file);
    if let Err(err) = tokio::fs::rename(&temporary, path).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(err.into());
    }
    Ok(())
}

fn approval_policy_from_permission_mode(mode: yode_core::permission::PermissionMode) -> String {
    match mode {
        yode_core::permission::PermissionMode::Bypass => "Always auto-approve",
        yode_core::permission::PermissionMode::Plan => "Never approve",
        _ => "On request",
    }
    .to_string()
}

fn sandbox_settings_from_permission_mode(mode: yode_core::permission::PermissionMode) -> String {
    match mode {
        yode_core::permission::PermissionMode::Plan => "Read only",
        yode_core::permission::PermissionMode::AcceptEdits
        | yode_core::permission::PermissionMode::Bypass => "Full write access",
        _ => "Restricted",
    }
    .to_string()
}

fn workspace_dependency_state_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".yode")
        .join("desktop-workspace-deps.json")
}

async fn load_workspace_dependency_state_async() -> bool {
    let path = workspace_dependency_state_path();
    let Ok(raw) = tokio::fs::read_to_string(path).await else {
        return true;
    };
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|value| {
            value
                .get("exposeDependencies")
                .and_then(|value| value.as_bool())
        })
        .unwrap_or(true)
}

async fn set_workspace_dependency_state_async(expose: bool) -> Result<()> {
    let path = workspace_dependency_state_path();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(
        path,
        serde_json::to_string_pretty(&json!({
            "exposeDependencies": expose,
            "updatedAt": Utc::now().to_rfc3339()
        }))?,
    )
    .await?;
    Ok(())
}

async fn workspace_diagnostic_checks(runtime: &DesktopRuntime) -> Result<Vec<DiagnosticCheck>> {
    let mut checks = Vec::new();
    let user_config = runtime.user_config_path();
    let project_config = runtime.project_config_path();
    checks.push(path_check("用户配置", &user_config, true));
    checks.push(path_check("项目配置", &project_config, false));
    checks.push(path_check("会话数据库", &runtime.db_path, true));
    checks.push(command_check("Node.js", "node", &["--version"]).await);
    checks.push(command_check("Python", "python3", &["--version"]).await);
    checks.push(command_check("Cargo", "cargo", &["--version"]).await);
    checks.push(path_check(
        "桌面端 package.json",
        &runtime
            .workspace_path
            .join("apps")
            .join("yode-desktop")
            .join("package.json"),
        true,
    ));
    let expose_dependencies = load_workspace_dependency_state_async().await;
    checks.push(DiagnosticCheck {
        name: "依赖项暴露".to_string(),
        status: if expose_dependencies { "ok" } else { "warn" }.to_string(),
        detail: if expose_dependencies {
            "已允许向工作区暴露 Node.js 与 Python 工具。"
        } else {
            "当前已关闭依赖项暴露。"
        }
        .to_string(),
    });
    Ok(checks)
}

fn path_check(name: &str, path: &Path, required: bool) -> DiagnosticCheck {
    let exists = path.exists();
    DiagnosticCheck {
        name: name.to_string(),
        status: if exists || !required { "ok" } else { "error" }.to_string(),
        detail: if exists {
            path.display().to_string()
        } else if required {
            format!("未找到 {}", path.display())
        } else {
            format!("未创建 {}", path.display())
        },
    }
}

async fn command_check(name: &str, command: &str, args: &[&str]) -> DiagnosticCheck {
    match tokio::process::Command::new(command)
        .args(args)
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            DiagnosticCheck {
                name: name.to_string(),
                status: "ok".to_string(),
                detail: if stdout.is_empty() { stderr } else { stdout },
            }
        }
        Ok(output) => DiagnosticCheck {
            name: name.to_string(),
            status: "error".to_string(),
            detail: format!("退出码 {}", output.status.code().unwrap_or(-1)),
        },
        Err(err) => DiagnosticCheck {
            name: name.to_string(),
            status: "error".to_string(),
            detail: err.to_string(),
        },
    }
}
