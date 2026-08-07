use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::plugin_trust::PluginTrustStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRegistry {
    plugins: Vec<Plugin>,
    diagnostics: Vec<PluginDiagnostic>,
}

impl PluginRegistry {
    pub fn discover(project_root: &Path) -> Self {
        discover_plugins_with_store(
            &project_root.join(".yode").join("plugins"),
            &PluginTrustStore::load(),
        )
    }

    pub async fn discover_async(project_root: &Path) -> Self {
        discover_plugins_with_store_async(
            &project_root.join(".yode").join("plugins"),
            &PluginTrustStore::load(),
        )
        .await
    }

    pub fn discover_dir(plugins_dir: &Path) -> Self {
        discover_plugins(plugins_dir)
    }

    pub async fn discover_dir_async(plugins_dir: &Path) -> Self {
        discover_plugins_async(plugins_dir).await
    }

    /// 使用显式信任存储发现插件（测试与桌面端注入自定义存储）。
    pub fn discover_dir_with_store(plugins_dir: &Path, store: &PluginTrustStore) -> Self {
        discover_plugins_with_store(plugins_dir, store)
    }

    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins
    }

    pub fn diagnostics(&self) -> &[PluginDiagnostic] {
        &self.diagnostics
    }

    pub fn get(&self, name: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|plugin| plugin.name == name)
    }

    pub fn enabled_plugins(&self) -> impl Iterator<Item = &Plugin> {
        self.plugins
            .iter()
            .filter(|plugin| plugin.trust == PluginTrustState::Enabled)
    }

    pub fn enabled_skill_paths(&self) -> Vec<PathBuf> {
        self.enabled_plugins()
            .flat_map(|plugin| plugin.contributions.skills.iter().cloned())
            .collect()
    }

    pub fn enabled_workflow_paths(&self) -> Vec<PathBuf> {
        self.enabled_plugins()
            .flat_map(|plugin| plugin.contributions.workflows.iter().cloned())
            .collect()
    }

    pub fn enabled_hook_paths(&self) -> Vec<PathBuf> {
        self.enabled_plugins()
            .flat_map(|plugin| plugin.contributions.hooks.iter().cloned())
            .collect()
    }

    pub fn enabled_command_paths(&self) -> Vec<PathBuf> {
        self.enabled_plugins()
            .flat_map(|plugin| plugin.contributions.commands.iter().cloned())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    pub name: String,
    pub description: Option<String>,
    pub trust: PluginTrustState,
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub contributions: PluginContributions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginContributions {
    pub skills: Vec<PathBuf>,
    pub workflows: Vec<PathBuf>,
    pub hooks: Vec<PathBuf>,
    pub commands: Vec<PathBuf>,
    pub mcp_servers: Vec<String>,
}

impl PluginContributions {
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
            && self.workflows.is_empty()
            && self.hooks.is_empty()
            && self.commands.is_empty()
            && self.mcp_servers.is_empty()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginTrustState {
    #[default]
    Installed,
    Enabled,
    Disabled,
    Blocked,
}

impl PluginTrustState {
    pub fn contributes(self) -> bool {
        self == Self::Enabled
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDiagnostic {
    pub plugin_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct PluginMcpDiscovery {
    pub servers: HashMap<String, crate::config::McpServerConfig>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginCommandDiscovery {
    pub commands: Vec<PluginCommandDefinition>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCommandDefinition {
    pub name: String,
    pub description: String,
    pub body: String,
    pub source: PathBuf,
}

#[derive(Debug, Deserialize)]
struct PluginCommandManifest {
    #[serde(default)]
    commands: Vec<PluginCommandEntry>,
}

#[derive(Debug, Deserialize)]
struct PluginCommandEntry {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PluginManifest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, alias = "state")]
    trust: Option<PluginTrustState>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    skills: Vec<String>,
    #[serde(default)]
    workflows: Vec<String>,
    #[serde(default)]
    hooks: Vec<String>,
    #[serde(default)]
    commands: Vec<String>,
    #[serde(default, alias = "mcp")]
    mcp_servers: Vec<String>,
}

fn discover_plugins(plugins_dir: &Path) -> PluginRegistry {
    discover_plugins_with_store(plugins_dir, &PluginTrustStore::default())
}

/// 发现插件并把信任状态绑定到仓库外的信任存储：
/// - 仓库内 manifest 的 `trust`/`enabled` 不是信任来源；
/// - 只有信任存储中存在与 canonical path + manifest 哈希匹配的记录时，
///   插件才可能处于 Enabled/Disabled/Blocked，否则一律 Installed（需授权）。
fn discover_plugins_with_store(plugins_dir: &Path, store: &PluginTrustStore) -> PluginRegistry {
    let mut plugins = Vec::new();
    let mut diagnostics = Vec::new();

    let mut entries = match std::fs::read_dir(plugins_dir) {
        Ok(entries) => entries
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.path())
            .collect::<Vec<_>>(),
        Err(_) => {
            return PluginRegistry {
                plugins,
                diagnostics,
            };
        }
    };
    entries.sort();

    for plugin_dir in entries {
        let manifest_path = plugin_dir.join("plugin.toml");
        if !manifest_path.is_file() {
            diagnostics.push(PluginDiagnostic {
                plugin_dir,
                manifest_path,
                message: "missing plugin.toml".to_string(),
            });
            continue;
        }

        match parse_plugin_manifest(&plugin_dir, &manifest_path) {
            Ok((mut plugin, legacy_trust_warning)) => {
                apply_trust_store(&mut plugin, &manifest_path, store);
                if let Some(warning) = legacy_trust_warning {
                    diagnostics.push(PluginDiagnostic {
                        plugin_dir: plugin_dir.clone(),
                        manifest_path: manifest_path.clone(),
                        message: warning,
                    });
                }
                plugins.push(plugin);
            }
            Err(message) => diagnostics.push(PluginDiagnostic {
                plugin_dir,
                manifest_path,
                message,
            }),
        }
    }

    PluginRegistry {
        plugins,
        diagnostics,
    }
}

async fn discover_plugins_async(plugins_dir: &Path) -> PluginRegistry {
    discover_plugins_with_store_async(plugins_dir, &PluginTrustStore::default()).await
}

async fn discover_plugins_with_store_async(
    plugins_dir: &Path,
    store: &PluginTrustStore,
) -> PluginRegistry {
    let mut plugins = Vec::new();
    let mut diagnostics = Vec::new();

    let mut entries = match tokio::fs::read_dir(plugins_dir).await {
        Ok(entries) => entries,
        Err(_) => {
            return PluginRegistry {
                plugins,
                diagnostics,
            };
        }
    };
    let mut plugin_dirs = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if tokio::fs::metadata(&path)
            .await
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            plugin_dirs.push(path);
        }
    }
    plugin_dirs.sort();

    for plugin_dir in plugin_dirs {
        let manifest_path = plugin_dir.join("plugin.toml");
        if tokio::fs::metadata(&manifest_path)
            .await
            .map(|metadata| !metadata.is_file())
            .unwrap_or(true)
        {
            diagnostics.push(PluginDiagnostic {
                plugin_dir,
                manifest_path,
                message: "missing plugin.toml".to_string(),
            });
            continue;
        }

        match parse_plugin_manifest_async(&plugin_dir, &manifest_path).await {
            Ok((mut plugin, legacy_trust_warning)) => {
                apply_trust_store(&mut plugin, &manifest_path, store);
                if let Some(warning) = legacy_trust_warning {
                    diagnostics.push(PluginDiagnostic {
                        plugin_dir: plugin_dir.clone(),
                        manifest_path: manifest_path.clone(),
                        message: warning,
                    });
                }
                plugins.push(plugin);
            }
            Err(message) => diagnostics.push(PluginDiagnostic {
                plugin_dir,
                manifest_path,
                message,
            }),
        }
    }

    PluginRegistry {
        plugins,
        diagnostics,
    }
}

/// 从仓库外信任存储解析插件的有效信任状态。manifest 哈希不匹配或
/// 无记录时回退到 Installed（必须重新授权）。
fn apply_trust_store(plugin: &mut Plugin, manifest_path: &Path, store: &PluginTrustStore) {
    let canonical = std::fs::canonicalize(&plugin.root).unwrap_or_else(|_| plugin.root.clone());
    let Ok(content) = std::fs::read_to_string(manifest_path) else {
        return;
    };
    let hash = PluginTrustStore::manifest_sha256(&content);
    if let Some(state) = store.state_for(&canonical, &hash) {
        plugin.trust = state;
    }
}

fn parse_plugin_manifest(
    plugin_dir: &Path,
    manifest_path: &Path,
) -> Result<(Plugin, Option<String>), String> {
    let content = std::fs::read_to_string(manifest_path)
        .map_err(|err| format!("failed to read plugin.toml: {err}"))?;
    parse_plugin_manifest_content(plugin_dir, manifest_path, &content)
}

async fn parse_plugin_manifest_async(
    plugin_dir: &Path,
    manifest_path: &Path,
) -> Result<(Plugin, Option<String>), String> {
    let content = tokio::fs::read_to_string(manifest_path)
        .await
        .map_err(|err| format!("failed to read plugin.toml: {err}"))?;
    parse_plugin_manifest_content(plugin_dir, manifest_path, &content)
}

fn parse_plugin_manifest_content(
    plugin_dir: &Path,
    manifest_path: &Path,
    content: &str,
) -> Result<(Plugin, Option<String>), String> {
    let manifest: PluginManifest =
        toml::from_str(content).map_err(|err| format!("invalid plugin.toml: {err}"))?;

    let name = manifest.name.trim();
    if name.is_empty() {
        return Err("plugin name is required".to_string());
    }

    // 仓库内 manifest 的 trust/enabled 字段不具权威性：信任状态一律从
    // 仓库外的信任存储解析（apply_trust_store），此处固定为 Installed。
    let trust = PluginTrustState::Installed;
    // 旧 manifest 里的自授信字段虽然不再生效，但暴露为可观察的诊断，
    // 让用户知道该字段被忽略，避免"明明 enabled 却不生效"的困惑。
    let legacy_trust_warning = if manifest.trust == Some(PluginTrustState::Enabled)
        || manifest.enabled == Some(true)
    {
        Some(
            "plugin.toml 中的 trust/enabled 字段已被忽略；信任状态由仓库外的 plugin-trust.toml 决定。"
                .to_string(),
        )
    } else {
        None
    };

    let contributions = PluginContributions {
        skills: resolve_contribution_paths(plugin_dir, &manifest.skills, "skills")?,
        workflows: resolve_contribution_paths(plugin_dir, &manifest.workflows, "workflows")?,
        hooks: resolve_contribution_paths(plugin_dir, &manifest.hooks, "hooks")?,
        commands: resolve_contribution_paths(plugin_dir, &manifest.commands, "commands")?,
        mcp_servers: normalized_names(&manifest.mcp_servers),
    };

    Ok((
        Plugin {
            name: name.to_string(),
            description: manifest
                .description
                .as_deref()
                .map(str::trim)
                .filter(|description| !description.is_empty())
                .map(ToString::to_string),
            trust,
            root: plugin_dir.to_path_buf(),
            manifest_path: manifest_path.to_path_buf(),
            contributions,
        },
        legacy_trust_warning,
    ))
}

fn resolve_contribution_paths(
    plugin_dir: &Path,
    values: &[String],
    field: &str,
) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();

    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }

        let path = Path::new(trimmed);
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(format!(
                "{field} contribution must stay inside the plugin: {trimmed}"
            ));
        }

        paths.push(plugin_dir.join(path));
    }

    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn normalized_names(values: &[String]) -> Vec<String> {
    let mut names = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

/// 设置插件信任状态：写入仓库外的信任存储（canonical path + manifest 哈希绑定），
/// 绝不修改仓库内的 plugin.toml。返回信任存储文件路径。
pub fn set_plugin_trust(
    project_root: &Path,
    name: &str,
    trust: PluginTrustState,
) -> Result<PathBuf, String> {
    let mut store = PluginTrustStore::load();
    let store_path =
        PluginTrustStore::default_path().ok_or_else(|| "无法定位用户主目录".to_string())?;
    set_plugin_trust_at(project_root, name, trust, &mut store, &store_path)
}

/// 将信任写入指定存储（测试与桌面端注入自定义位置）。
pub fn set_plugin_trust_at(
    project_root: &Path,
    name: &str,
    trust: PluginTrustState,
    store: &mut PluginTrustStore,
    store_path: &Path,
) -> Result<PathBuf, String> {
    let registry =
        PluginRegistry::discover_dir_with_store(&project_root.join(".yode").join("plugins"), store);
    let plugin = registry
        .get(name)
        .ok_or_else(|| format!("Plugin '{name}' not found."))?;
    let content = std::fs::read_to_string(&plugin.manifest_path)
        .map_err(|err| format!("failed to read {}: {err}", plugin.manifest_path.display()))?;
    let hash = PluginTrustStore::manifest_sha256(&content);
    let canonical = std::fs::canonicalize(&plugin.root)
        .map_err(|err| format!("无法解析插件路径 {}: {err}", plugin.root.display()))?;
    store.set_at(&canonical, hash, trust, store_path)?;
    Ok(store_path.to_path_buf())
}

pub fn discover_plugin_mcp_servers(project_root: &Path) -> PluginMcpDiscovery {
    discover_plugin_mcp_servers_with_store(project_root, &PluginTrustStore::load())
}

pub fn discover_plugin_mcp_servers_with_store(
    project_root: &Path,
    store: &PluginTrustStore,
) -> PluginMcpDiscovery {
    let mut discovery = PluginMcpDiscovery::default();
    for plugin in
        PluginRegistry::discover_dir_with_store(&project_root.join(".yode").join("plugins"), store)
            .enabled_plugins()
    {
        for contribution in &plugin.contributions.mcp_servers {
            let path = Path::new(contribution);
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }
            if path.is_absolute()
                || path
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
            {
                discovery.diagnostics.push(format!(
                    "MCP contribution must stay inside plugin '{}': {}",
                    plugin.name, contribution
                ));
                continue;
            }
            let path = plugin.root.join(path);
            match std::fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {}", path.display(), err))
                .and_then(|content| {
                    toml::from_str::<crate::config::McpConfig>(&content)
                        .map_err(|err| format!("invalid MCP manifest {}: {}", path.display(), err))
                }) {
                Ok(config) => {
                    for (server, config) in config.servers {
                        discovery.servers.entry(server).or_insert(config);
                    }
                }
                Err(message) => discovery.diagnostics.push(message),
            }
        }
    }
    discovery
}

pub async fn discover_plugin_mcp_servers_async(project_root: &Path) -> PluginMcpDiscovery {
    let mut discovery = PluginMcpDiscovery::default();
    let registry = PluginRegistry::discover_async(project_root).await;
    for plugin in registry.enabled_plugins() {
        for contribution in &plugin.contributions.mcp_servers {
            let path = Path::new(contribution);
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }
            if path.is_absolute()
                || path
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
            {
                discovery.diagnostics.push(format!(
                    "MCP contribution must stay inside plugin '{}': {}",
                    plugin.name, contribution
                ));
                continue;
            }
            let path = plugin.root.join(path);
            match tokio::fs::read_to_string(&path)
                .await
                .map_err(|err| format!("failed to read {}: {}", path.display(), err))
                .and_then(|content| {
                    toml::from_str::<crate::config::McpConfig>(&content)
                        .map_err(|err| format!("invalid MCP manifest {}: {}", path.display(), err))
                }) {
                Ok(config) => {
                    for (server, config) in config.servers {
                        discovery.servers.entry(server).or_insert(config);
                    }
                }
                Err(message) => discovery.diagnostics.push(message),
            }
        }
    }
    discovery
}

pub fn discover_plugin_commands(project_root: &Path) -> PluginCommandDiscovery {
    discover_plugin_commands_with_store(project_root, &PluginTrustStore::load())
}

pub fn discover_plugin_commands_with_store(
    project_root: &Path,
    store: &PluginTrustStore,
) -> PluginCommandDiscovery {
    let mut discovery = PluginCommandDiscovery::default();
    for path in
        PluginRegistry::discover_dir_with_store(&project_root.join(".yode").join("plugins"), store)
            .enabled_command_paths()
    {
        for command_path in expand_toml_contribution(path) {
            match std::fs::read_to_string(&command_path)
                .map_err(|err| format!("failed to read {}: {}", command_path.display(), err))
                .and_then(|content| {
                    toml::from_str::<PluginCommandManifest>(&content).map_err(|err| {
                        format!(
                            "invalid command manifest {}: {}",
                            command_path.display(),
                            err
                        )
                    })
                }) {
                Ok(manifest) => {
                    for entry in manifest.commands {
                        match normalize_plugin_command(entry, &command_path) {
                            Ok(command) => discovery.commands.push(command),
                            Err(message) => discovery.diagnostics.push(message),
                        }
                    }
                }
                Err(message) => discovery.diagnostics.push(message),
            }
        }
    }
    discovery
}

pub async fn discover_plugin_commands_async(project_root: &Path) -> PluginCommandDiscovery {
    let mut discovery = PluginCommandDiscovery::default();
    let registry = PluginRegistry::discover_async(project_root).await;
    for path in registry.enabled_command_paths() {
        for command_path in expand_toml_contribution_async(path).await {
            match tokio::fs::read_to_string(&command_path)
                .await
                .map_err(|err| format!("failed to read {}: {}", command_path.display(), err))
                .and_then(|content| {
                    toml::from_str::<PluginCommandManifest>(&content).map_err(|err| {
                        format!(
                            "invalid command manifest {}: {}",
                            command_path.display(),
                            err
                        )
                    })
                }) {
                Ok(manifest) => {
                    for entry in manifest.commands {
                        match normalize_plugin_command(entry, &command_path) {
                            Ok(command) => discovery.commands.push(command),
                            Err(message) => discovery.diagnostics.push(message),
                        }
                    }
                }
                Err(message) => discovery.diagnostics.push(message),
            }
        }
    }
    discovery
}

fn normalize_plugin_command(
    entry: PluginCommandEntry,
    source: &Path,
) -> Result<PluginCommandDefinition, String> {
    let name = entry.name.trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(format!(
            "invalid plugin command name '{}' in {}",
            entry.name,
            source.display()
        ));
    }

    let description = entry.description.trim();
    let body = entry
        .message
        .as_deref()
        .or(entry.prompt.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(description);
    if body.is_empty() {
        return Err(format!(
            "plugin command '{}' in {} needs message, prompt, or description",
            name,
            source.display()
        ));
    }

    Ok(PluginCommandDefinition {
        name: name.to_string(),
        description: if description.is_empty() {
            body.chars().take(80).collect()
        } else {
            description.to_string()
        },
        body: body.to_string(),
        source: source.to_path_buf(),
    })
}

fn expand_toml_contribution(path: PathBuf) -> Vec<PathBuf> {
    if path.is_dir() {
        let mut paths = std::fs::read_dir(path)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
            .collect::<Vec<_>>();
        paths.sort();
        return paths;
    }

    if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
        vec![path]
    } else {
        Vec::new()
    }
}

async fn expand_toml_contribution_async(path: PathBuf) -> Vec<PathBuf> {
    if tokio::fs::metadata(&path)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        let mut entries = match tokio::fs::read_dir(path).await {
            Ok(entries) => entries,
            Err(_) => return Vec::new(),
        };
        let mut paths = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
                paths.push(path);
            }
        }
        paths.sort();
        return paths;
    }

    if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
        vec![path]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_trust::{PluginTrustEntry, PluginTrustStore};

    fn write_manifest(dir: &Path, plugin: &str, manifest: &str) {
        let plugin_dir = dir.join(plugin);
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("plugin.toml"), manifest).unwrap();
    }

    /// 构造一个把指定插件标记为指定信任状态的仓库外信任存储（绑定 canonical path + 哈希）。
    fn trust_store_for(dir: &Path, plugin: &str, trust: PluginTrustState) -> PluginTrustStore {
        let mut store = PluginTrustStore::default();
        let plugin_dir = dir.join(plugin);
        let manifest = std::fs::read_to_string(plugin_dir.join("plugin.toml")).unwrap();
        let canonical = std::fs::canonicalize(&plugin_dir).unwrap();
        store.plugins.insert(
            canonical.to_string_lossy().to_string(),
            PluginTrustEntry {
                path: canonical,
                manifest_sha256: PluginTrustStore::manifest_sha256(&manifest),
                trust,
            },
        );
        store
    }

    #[test]
    fn discovers_plugin_manifests_deterministically() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            "zeta",
            r#"
name = "zeta"
trust = "enabled"
skills = ["skills/zeta/SKILL.md"]
workflows = ["workflows/zeta.json"]
mcp_servers = ["docs", "docs", "review"]
"#,
        );
        write_manifest(
            dir.path(),
            "alpha",
            r#"
name = "alpha"
description = "Alpha plugin"
enabled = false
hooks = ["hooks/alpha.toml"]
commands = ["commands/alpha.toml"]
"#,
        );

        let registry = PluginRegistry::discover_dir(dir.path());
        let names = registry
            .plugins()
            .iter()
            .map(|plugin| plugin.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["alpha", "zeta"]);
        // 没有仓库外信任记录时，manifest 里的 trust/enabled 不具权威性：
        // 所有插件都是 Installed（需要用户授权）。
        assert_eq!(
            registry.get("alpha").unwrap().trust,
            PluginTrustState::Installed
        );
        assert_eq!(
            registry.get("zeta").unwrap().trust,
            PluginTrustState::Installed
        );
        // 自授信字段被忽略时要暴露诊断
        assert!(registry
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.message.contains("trust/enabled 字段已被忽略") }));
        assert_eq!(
            registry.get("zeta").unwrap().contributions.mcp_servers,
            vec!["docs".to_string(), "review".to_string()]
        );
    }

    #[test]
    fn reports_missing_and_invalid_manifests() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("missing")).unwrap();
        write_manifest(dir.path(), "broken", "name = ");

        let registry = PluginRegistry::discover_dir(dir.path());
        let messages = registry
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();

        assert_eq!(registry.plugins().len(), 0);
        assert!(messages.contains(&"missing plugin.toml"));
        assert!(messages
            .iter()
            .any(|message| message.starts_with("invalid plugin.toml")));
    }

    #[test]
    fn rejects_contributions_outside_plugin_root() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            "escape",
            r#"
name = "escape"
trust = "enabled"
skills = ["../shared/SKILL.md"]
"#,
        );

        let registry = PluginRegistry::discover_dir(dir.path());

        assert!(registry.plugins().is_empty());
        assert!(registry.diagnostics()[0]
            .message
            .contains("skills contribution must stay inside the plugin"));
    }

    #[test]
    fn enabled_plugins_only_returns_enabled_trust_state() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), "installed", r#"name = "installed""#);
        write_manifest(
            dir.path(),
            "enabled",
            r#"
name = "enabled"
trust = "enabled"
"#,
        );
        write_manifest(
            dir.path(),
            "blocked",
            r#"
name = "blocked"
trust = "blocked"
"#,
        );

        // 无信任存储：没有任何插件被启用（manifest 自授信不生效）
        let registry = PluginRegistry::discover_dir(dir.path());
        assert_eq!(registry.enabled_plugins().count(), 0);

        // 有信任存储：只有 Enabled 记录贡献，Blocked/Installed 不贡献
        let mut store = trust_store_for(dir.path(), "enabled", PluginTrustState::Enabled);
        let blocked_dir = dir.path().join("blocked");
        let manifest = std::fs::read_to_string(blocked_dir.join("plugin.toml")).unwrap();
        let canonical = std::fs::canonicalize(&blocked_dir).unwrap();
        store.plugins.insert(
            canonical.to_string_lossy().to_string(),
            PluginTrustEntry {
                path: canonical,
                manifest_sha256: PluginTrustStore::manifest_sha256(&manifest),
                trust: PluginTrustState::Blocked,
            },
        );
        let registry = PluginRegistry::discover_dir_with_store(dir.path(), &store);
        let enabled = registry
            .enabled_plugins()
            .map(|plugin| plugin.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(enabled, vec!["enabled"]);
        assert_eq!(
            registry.get("blocked").unwrap().trust,
            PluginTrustState::Blocked
        );
    }

    #[test]
    fn set_plugin_trust_writes_external_store_not_repo_manifest() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            &dir.path().join(".yode").join("plugins"),
            "demo",
            r#"
name = "demo"
enabled = false
skills = ["skills/demo/SKILL.md"]
"#,
        );

        let store_path = dir.path().join("plugin-trust.toml");
        let mut store = PluginTrustStore::default();
        let written = set_plugin_trust_at(
            dir.path(),
            "demo",
            PluginTrustState::Enabled,
            &mut store,
            &store_path,
        )
        .unwrap();
        assert_eq!(written, store_path);

        // 仓库内 manifest 必须保持原样：不被写入 trust/enabled
        let manifest = std::fs::read_to_string(
            dir.path()
                .join(".yode")
                .join("plugins")
                .join("demo")
                .join("plugin.toml"),
        )
        .unwrap();
        assert!(!manifest.contains("trust"));
        assert!(manifest.contains("enabled = false"));

        // 使用该信任存储发现时插件处于 Enabled
        let registry = PluginRegistry::discover_dir_with_store(
            &dir.path().join(".yode").join("plugins"),
            &store,
        );
        assert_eq!(
            registry.get("demo").unwrap().trust,
            PluginTrustState::Enabled
        );

        // 篡改 manifest 后既有信任失效，插件回到 Installed
        std::fs::write(
            dir.path()
                .join(".yode")
                .join("plugins")
                .join("demo")
                .join("plugin.toml"),
            "name = \"demo\"\n",
        )
        .unwrap();
        let registry = PluginRegistry::discover_dir_with_store(
            &dir.path().join(".yode").join("plugins"),
            &store,
        );
        assert_eq!(
            registry.get("demo").unwrap().trust,
            PluginTrustState::Installed
        );
    }

    #[test]
    fn malicious_manifest_cannot_self_enable() {
        let dir = tempfile::tempdir().unwrap();
        // 恶意仓库：plugin.toml 里直接写 trust = "enabled"
        write_manifest(
            &dir.path().join(".yode").join("plugins"),
            "evil",
            r#"
name = "evil"
trust = "enabled"
enabled = true
hooks = ["hooks/evil.toml"]
"#,
        );

        // 没有仓库外信任记录 => 即使仓库自授信也不启用
        let registry = PluginRegistry::discover(dir.path());
        assert_eq!(registry.enabled_plugins().count(), 0);
        assert_eq!(
            registry.get("evil").unwrap().trust,
            PluginTrustState::Installed
        );
        assert!(registry
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message.contains("已被忽略")));
    }

    #[test]
    fn discovers_enabled_plugin_mcp_server_manifests() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            &dir.path().join(".yode").join("plugins"),
            "demo",
            r#"
name = "demo"
trust = "enabled"
mcp_servers = ["mcp/servers.toml", "inventory-only"]
"#,
        );
        let mcp_dir = dir
            .path()
            .join(".yode")
            .join("plugins")
            .join("demo")
            .join("mcp");
        std::fs::create_dir_all(&mcp_dir).unwrap();
        std::fs::write(
            mcp_dir.join("servers.toml"),
            r#"
[servers.plugin_docs]
command = "yode-mcp-demo"
args = ["--stdio"]
"#,
        )
        .unwrap();

        let store = trust_store_for(
            &dir.path().join(".yode").join("plugins"),
            "demo",
            PluginTrustState::Enabled,
        );
        let discovery = discover_plugin_mcp_servers_with_store(dir.path(), &store);

        assert!(discovery.diagnostics.is_empty());
        let server = discovery.servers.get("plugin_docs").unwrap();
        assert_eq!(server.command, "yode-mcp-demo");
        assert_eq!(server.args, vec!["--stdio".to_string()]);
    }

    #[test]
    fn disabled_plugin_mcp_servers_are_not_discovered() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            &dir.path().join(".yode").join("plugins"),
            "demo",
            r#"
name = "demo"
trust = "disabled"
mcp_servers = ["mcp/servers.toml"]
"#,
        );
        let mcp_dir = dir
            .path()
            .join(".yode")
            .join("plugins")
            .join("demo")
            .join("mcp");
        std::fs::create_dir_all(&mcp_dir).unwrap();
        std::fs::write(
            mcp_dir.join("servers.toml"),
            r#"
[servers.plugin_docs]
command = "yode-mcp-demo"
"#,
        )
        .unwrap();

        // 无信任记录 => 插件未被启用，MCP 服务器不加载
        let store = trust_store_for(
            &dir.path().join(".yode").join("plugins"),
            "demo",
            PluginTrustState::Installed,
        );
        let discovery = discover_plugin_mcp_servers_with_store(dir.path(), &store);

        assert!(discovery.servers.is_empty());
        assert!(discovery.diagnostics.is_empty());
    }

    #[test]
    fn discovers_enabled_plugin_commands() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            &dir.path().join(".yode").join("plugins"),
            "demo",
            r#"
name = "demo"
trust = "enabled"
commands = ["commands/demo.toml"]
"#,
        );
        let command_dir = dir
            .path()
            .join(".yode")
            .join("plugins")
            .join("demo")
            .join("commands");
        std::fs::create_dir_all(&command_dir).unwrap();
        std::fs::write(
            command_dir.join("demo.toml"),
            r#"
[[commands]]
name = "demo-review"
description = "Run plugin review prompt"
prompt = "Review this plugin contribution."
"#,
        )
        .unwrap();

        let store = trust_store_for(
            &dir.path().join(".yode").join("plugins"),
            "demo",
            PluginTrustState::Enabled,
        );
        let discovery = discover_plugin_commands_with_store(dir.path(), &store);

        assert!(discovery.diagnostics.is_empty());
        assert_eq!(discovery.commands.len(), 1);
        assert_eq!(discovery.commands[0].name, "demo-review");
        assert_eq!(
            discovery.commands[0].body,
            "Review this plugin contribution."
        );
    }

    #[test]
    fn disabled_plugin_commands_are_not_discovered() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            &dir.path().join(".yode").join("plugins"),
            "demo",
            r#"
name = "demo"
trust = "disabled"
commands = ["commands/demo.toml"]
"#,
        );
        let command_dir = dir
            .path()
            .join(".yode")
            .join("plugins")
            .join("demo")
            .join("commands");
        std::fs::create_dir_all(&command_dir).unwrap();
        std::fs::write(
            command_dir.join("demo.toml"),
            r#"
[[commands]]
name = "demo-review"
description = "Run plugin review prompt"
"#,
        )
        .unwrap();

        let store = trust_store_for(
            &dir.path().join(".yode").join("plugins"),
            "demo",
            PluginTrustState::Installed,
        );
        let discovery = discover_plugin_commands_with_store(dir.path(), &store);

        assert!(discovery.commands.is_empty());
        assert!(discovery.diagnostics.is_empty());
    }
}
