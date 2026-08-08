use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
            if scope.to_lowercase().contains("project") {
                // 项目配置只允许包含可共享字段，绝不写入 API key / token
                let config = self
                    .config
                    .lock()
                    .map_err(|_| anyhow::anyhow!("config lock poisoned"))?
                    .clone();
                save_project_config_to_path_async(&config, &path).await?;
            } else {
                // 事务创建：在文件锁内再次确认文件仍不存在，避免并发进程新建的
                // 配置被本进程的过期完整快照覆盖。
                let config = self
                    .config
                    .lock()
                    .map_err(|_| anyhow::anyhow!("config lock poisoned"))?
                    .clone();
                Config::update_config_file(&path, move |existing| {
                    if let Some(raw) = existing {
                        return Ok((false, raw.to_vec()));
                    }
                    let serialized = toml::to_string_pretty(&config)?;
                    Ok((true, serialized.into_bytes()))
                })?;
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

pub(super) async fn load_desktop_config(user_config_path: &Path) -> Result<Config> {
    // 未建立仓库外信任记录前，桌面端不执行任何仓库配置覆盖。尤其不能让
    // `.yode/config.toml` 改写 endpoint/API key、权限模式、MCP 或 Hooks。
    Config::load_with_overrides_async(Some(user_config_path), None).await
}

/// 同一进程内所有用户配置事务的串行化锁：把“文件锁内修改 -> 刷新内存快照”
/// 作为一个原子临界区，两个并发 RPC 不会因交错读取令内存状态倒退；
/// 跨进程安全性仍由核心层文件锁保证。
static USER_CONFIG_UPDATE_LOCK: Mutex<()> = Mutex::new(());

impl DesktopRuntime {
    /// 统一用户配置事务入口（跨进程安全、进程内串行）。
    ///
    /// 在核心层文件锁保护下，以磁盘上最新的用户配置为基础应用“窄修改”并原子写回，
    /// 因此两个应用实例并发更新不同配置域时不会基于过期完整快照互相覆盖；前端表单
    /// 不承载的 API key 与 MCP auth 等字段以及未知顶层/嵌套字段也因无损文档事务而
    /// 完整保留。
    ///
    /// 写回成功后才刷新内存配置快照（重新从磁盘加载，不触发任何迁移写回），且整个
    /// “锁内修改 -> 刷新内存”处于进程内互斥临界区中：同一运行时并发更新不会令内存
    /// 状态倒退。失败时内存与磁盘均保持原状。调用方如需更新 provider registry、
    /// MCP tooling、权限模式等派生状态，必须在收到 `Ok` 之后再进行，不得在失败路径
    /// 污染任何内存状态。
    pub fn update_user_config<T, F>(&self, update: F) -> Result<T>
    where
        F: FnOnce(&mut Config) -> Result<T>,
    {
        let _guard = USER_CONFIG_UPDATE_LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("user config update lock poisoned"))?;
        let path = self.user_config_path();
        let result = Config::update_user_config_file(&path, update)?;
        let fresh = Config::load_with_overrides(Some(&path), None)
            .with_context(|| "用户配置写入成功后刷新内存快照失败")?;
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        *config = fresh;
        Ok(result)
    }
}

/// 写项目级共享配置：只写脱敏后的可共享字段（不含 API key 与疑似密钥环境变量）。
/// 用户级配置仍然写入完整内容。
pub(super) async fn save_project_config_to_path_async(config: &Config, path: &Path) -> Result<()> {
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
    Config::write_config_file_async(path, serialized.as_bytes()).await
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
