use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use std::path::{Path, PathBuf};

use yode_core::config::Config;

use super::{settings_runtime::open_with_destination, DesktopRuntime};
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
        })
    }

    pub async fn configuration_update(
        &self,
        request: ConfigurationUpdateRequest,
    ) -> Result<ConfigurationState> {
        let scope = if request.scope.to_lowercase().contains("project") {
            ConfigScope::Project
        } else {
            ConfigScope::User
        };
        let permission_mode =
            permission_mode_from_configuration(&request.approval_policy, &request.sandbox_settings);
        {
            let mut runtime_mode = self
                .permission_mode
                .lock()
                .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?;
            *runtime_mode = permission_mode.to_string();
        }
        let config_path = self.config_path_for_scope(scope);
        let config_to_save = {
            let mut config = self
                .config
                .lock()
                .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
            config.permissions.default_mode = Some(permission_mode.to_string());
            config.clone()
        };
        save_config_to_path_async(&config_to_save, &config_path).await?;
        set_workspace_dependency_state_async(request.expose_dependencies).await?;
        Ok(ConfigurationState {
            scope: request.scope,
            approval_policy: request.approval_policy,
            sandbox_settings: request.sandbox_settings,
            expose_dependencies: request.expose_dependencies,
            config_path: self.user_config_path().display().to_string(),
            project_config_path: self.project_config_path().display().to_string(),
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
            save_config_to_path_async(&config, &path).await?;
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

    fn config_path_for_scope(&self, scope: ConfigScope) -> PathBuf {
        match scope {
            ConfigScope::User => self.user_config_path(),
            ConfigScope::Project => self.project_config_path(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ConfigScope {
    User,
    Project,
}

pub(super) async fn load_desktop_config(workspace_path: &Path) -> Result<Config> {
    let project_config = workspace_path.join(".yode").join("config.toml");
    if tokio::fs::try_exists(&project_config).await? {
        Config::load_from_async(Some(&project_config)).await
    } else {
        Config::load_async().await
    }
}

pub(super) async fn save_config_to_path_async(config: &Config, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, toml::to_string_pretty(config)?).await?;
    Ok(())
}

fn permission_mode_from_configuration(
    approval_policy: &str,
    sandbox_settings: &str,
) -> yode_core::permission::PermissionMode {
    let approval = approval_policy.to_lowercase();
    if approval.contains("always") || approval.contains("始终") {
        return yode_core::permission::PermissionMode::Bypass;
    }
    if approval.contains("never") || approval.contains("从不") {
        return yode_core::permission::PermissionMode::Plan;
    }

    let sandbox = sandbox_settings.to_lowercase();
    if sandbox.contains("read only") || sandbox.contains("只读") {
        yode_core::permission::PermissionMode::Plan
    } else if sandbox.contains("full") || sandbox.contains("读写") {
        yode_core::permission::PermissionMode::AcceptEdits
    } else {
        yode_core::permission::PermissionMode::Default
    }
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
