use std::path::Path;

use yode_core::config::Config;
use yode_core::permission::{
    PermissionConfig, PermissionManager, PermissionRuleConfig, RuleSource,
};

/// 桌面端权限分层加载，与 CLI（src/app_bootstrap/session_restore.rs）保持同一套语义：
/// - 用户配置 ~/.yode/config.toml（UserConfig）
/// - 受管策略 ~/.yode/managed-config.toml（ManagedConfig，存在时参与）
/// - 项目配置 <workdir>/.yode/config.toml（ProjectConfig）
/// - 本地覆盖 <workdir>/.yode/config.local.toml（LocalConfig）
///
/// 规则优先级由 RuleSource 决定（Managed > Local > Project > User）；
/// 只有用户配置和受管策略可以设置 default_mode；仓库内配置（Project/Local）
/// 只能贡献 always_deny 收紧规则，永远不能放宽权限或切换模式。
pub(super) fn configure_desktop_permissions(config: &Config, workdir: &Path) -> PermissionManager {
    let mut permissions =
        PermissionManager::from_confirmation_list(config.tools.require_confirmation.clone());
    for (source, layer) in permission_layers(config, workdir) {
        // 仓库内配置只能收紧（deny），不能切换模式或添加 allow/ask。
        if !matches!(source, RuleSource::ProjectConfig | RuleSource::LocalConfig) {
            if let Some(mode_str) = &layer.default_mode {
                if let Ok(mode) = mode_str.parse::<yode_core::permission::PermissionMode>() {
                    permissions.set_mode(mode);
                }
            }
        }
        let rules = layer.to_rules(source);
        if !rules.is_empty() {
            permissions.add_rules(rules);
        }
    }
    permissions
}

fn permission_layers(config: &Config, workdir: &Path) -> Vec<(RuleSource, PermissionConfig)> {
    permission_layers_with_home(config, workdir, dirs::home_dir())
}

fn permission_layers_with_home(
    config: &Config,
    workdir: &Path,
    home: Option<std::path::PathBuf>,
) -> Vec<(RuleSource, PermissionConfig)> {
    use yode_core::permission::RuleSource;

    let mut layers = vec![(
        RuleSource::UserConfig,
        permission_config_from_runtime_config(config),
    )];

    let managed_path = home
        .map(|home| home.join(".yode").join("managed-config.toml"))
        .filter(|path| path.exists());
    if let Some(path) = managed_path.as_deref() {
        if let Some(layer) = load_full_permission_config_from_path(path) {
            layers.push((RuleSource::ManagedConfig, layer));
        }
    }

    let project_path = workdir.join(".yode").join("config.toml");
    if let Some(layer) = load_tightening_permission_config_from_path(&project_path) {
        layers.push((RuleSource::ProjectConfig, layer));
    }

    let local_path = workdir.join(".yode").join("config.local.toml");
    if let Some(layer) = load_tightening_permission_config_from_path(&local_path) {
        layers.push((RuleSource::LocalConfig, layer));
    }

    layers
}

fn permission_config_from_runtime_config(config: &Config) -> PermissionConfig {
    PermissionConfig {
        default_mode: config.permissions.default_mode.clone(),
        always_allow: config
            .permissions
            .always_allow
            .iter()
            .map(permission_rule_entry_to_config)
            .collect(),
        always_ask: config
            .permissions
            .always_ask
            .iter()
            .map(permission_rule_entry_to_config)
            .collect(),
        always_deny: config
            .permissions
            .always_deny
            .iter()
            .map(permission_rule_entry_to_config)
            .collect(),
    }
}

fn permission_rule_entry_to_config(
    entry: &yode_core::config::PermissionRuleEntry,
) -> PermissionRuleConfig {
    PermissionRuleConfig {
        tool: entry.tool.clone(),
        category: entry.category.clone(),
        pattern: entry.pattern.clone(),
        description: entry.description.clone(),
    }
}

/// 加载完整权限配置（用户配置、受管策略）：保留 default_mode 与全部规则。
fn load_full_permission_config_from_path(path: &Path) -> Option<PermissionConfig> {
    if !path.exists() {
        return None;
    }
    Config::load_from(Some(path))
        .ok()
        .map(|config| permission_config_from_runtime_config(&config))
}

/// 仓库内配置（项目/本地覆盖）只能收紧：只保留 always_deny 规则，
/// 忽略 default_mode、always_allow 与 always_ask。
fn load_tightening_permission_config_from_path(path: &Path) -> Option<PermissionConfig> {
    if !path.exists() {
        return None;
    }
    Config::load_from(Some(path))
        .ok()
        .map(|config| PermissionConfig {
            default_mode: None,
            always_allow: Vec::new(),
            always_ask: Vec::new(),
            always_deny: config
                .permissions
                .always_deny
                .iter()
                .map(permission_rule_entry_to_config)
                .collect(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use yode_core::permission::PermissionAction;

    fn base_config_toml() -> &'static str {
        r#"
[llm]
default_provider = "openai"
default_model = "gpt-4o"

[tools]
bash_timeout = 120
require_confirmation = ["bash"]

[session]
db_path = ""

[ui]
language = "zh-CN"
theme = "dark"
"#
    }

    #[test]
    fn desktop_permissions_merge_user_project_and_local_layers() {
        let dir =
            std::env::temp_dir().join(format!("yode-desktop-perm-layer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yode")).unwrap();
        std::fs::write(
            dir.join(".yode").join("config.toml"),
            format!(
                r#"{base}
[permissions]
default_mode = "plan"

[[permissions.always_deny]]
category = "write"
"#,
                base = base_config_toml()
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join(".yode").join("config.local.toml"),
            format!(
                r#"{base}
[permissions]
default_mode = "accept-edits"

[[permissions.always_allow]]
tool = "write_file"
"#,
                base = base_config_toml()
            ),
        )
        .unwrap();

        let base: Config = toml::from_str(base_config_toml()).unwrap();
        let permissions = configure_desktop_permissions(&base, &dir);

        // 仓库内配置（project/local）不能切换权限模式：即使写入了 plan / accept-edits，
        // 有效模式仍保持 Default，必须由用户或受管策略决定。
        assert_eq!(permissions.mode(), yode_core::PermissionMode::Default);
        // 仓库内配置不能放宽权限：local 的 always_allow 被忽略，项目 deny 仍然生效。
        assert_eq!(
            permissions.explain_with_content("write_file", None).action,
            PermissionAction::Deny
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn desktop_permissions_load_managed_layer() {
        // 使用隔离的临时 HOME，不触碰真实用户配置
        let home_dir =
            std::env::temp_dir().join(format!("yode-desktop-managed-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home_dir);
        std::fs::create_dir_all(home_dir.join(".yode")).unwrap();
        std::fs::write(
            home_dir.join(".yode").join("managed-config.toml"),
            format!(
                r#"{base}
[[permissions.always_deny]]
tool = "read_file"
"#,
                base = base_config_toml()
            ),
        )
        .unwrap();

        let base: Config = toml::from_str(base_config_toml()).unwrap();
        let mut layers = super::permission_layers_with_home(
            &base,
            std::path::Path::new("/tmp/nonexistent-dir"),
            Some(home_dir.clone()),
        );
        // 受管策略优先级最高：read_file 被拒绝
        let permissions = {
            let mut permissions =
                PermissionManager::from_confirmation_list(base.tools.require_confirmation.clone());
            for (source, layer) in layers.drain(..) {
                if let Some(mode_str) = &layer.default_mode {
                    if let Ok(mode) = mode_str.parse::<yode_core::permission::PermissionMode>() {
                        permissions.set_mode(mode);
                    }
                }
                permissions.add_rules(layer.to_rules(source));
            }
            permissions
        };
        assert_eq!(
            permissions.explain_with_content("read_file", None).action,
            PermissionAction::Deny
        );
        let _ = std::fs::remove_dir_all(&home_dir);
    }
}
