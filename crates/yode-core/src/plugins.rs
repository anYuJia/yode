use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRegistry {
    plugins: Vec<Plugin>,
    diagnostics: Vec<PluginDiagnostic>,
}

impl PluginRegistry {
    pub fn discover(project_root: &Path) -> Self {
        discover_plugins(&project_root.join(".yode").join("plugins"))
    }

    pub async fn discover_async(project_root: &Path) -> Self {
        discover_plugins_async(&project_root.join(".yode").join("plugins")).await
    }

    pub fn discover_dir(plugins_dir: &Path) -> Self {
        discover_plugins(plugins_dir)
    }

    pub async fn discover_dir_async(plugins_dir: &Path) -> Self {
        discover_plugins_async(plugins_dir).await
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
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
            Ok(plugin) => plugins.push(plugin),
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
            Ok(plugin) => plugins.push(plugin),
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

fn parse_plugin_manifest(plugin_dir: &Path, manifest_path: &Path) -> Result<Plugin, String> {
    let content = std::fs::read_to_string(manifest_path)
        .map_err(|err| format!("failed to read plugin.toml: {err}"))?;
    parse_plugin_manifest_content(plugin_dir, manifest_path, &content)
}

async fn parse_plugin_manifest_async(
    plugin_dir: &Path,
    manifest_path: &Path,
) -> Result<Plugin, String> {
    let content = tokio::fs::read_to_string(manifest_path)
        .await
        .map_err(|err| format!("failed to read plugin.toml: {err}"))?;
    parse_plugin_manifest_content(plugin_dir, manifest_path, &content)
}

fn parse_plugin_manifest_content(
    plugin_dir: &Path,
    manifest_path: &Path,
    content: &str,
) -> Result<Plugin, String> {
    let manifest: PluginManifest =
        toml::from_str(content).map_err(|err| format!("invalid plugin.toml: {err}"))?;

    let name = manifest.name.trim();
    if name.is_empty() {
        return Err("plugin name is required".to_string());
    }

    let trust = manifest.trust.unwrap_or(match manifest.enabled {
        Some(true) => PluginTrustState::Enabled,
        Some(false) => PluginTrustState::Disabled,
        None => PluginTrustState::Installed,
    });

    let contributions = PluginContributions {
        skills: resolve_contribution_paths(plugin_dir, &manifest.skills, "skills")?,
        workflows: resolve_contribution_paths(plugin_dir, &manifest.workflows, "workflows")?,
        hooks: resolve_contribution_paths(plugin_dir, &manifest.hooks, "hooks")?,
        commands: resolve_contribution_paths(plugin_dir, &manifest.commands, "commands")?,
        mcp_servers: normalized_names(&manifest.mcp_servers),
    };

    Ok(Plugin {
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
    })
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

pub fn set_plugin_trust(
    project_root: &Path,
    name: &str,
    trust: PluginTrustState,
) -> Result<PathBuf, String> {
    let registry = PluginRegistry::discover(project_root);
    let plugin = registry
        .get(name)
        .ok_or_else(|| format!("Plugin '{name}' not found."))?;
    let manifest_path = plugin.manifest_path.clone();
    let content = std::fs::read_to_string(&manifest_path)
        .map_err(|err| format!("failed to read {}: {err}", manifest_path.display()))?;
    let mut value = content
        .parse::<toml::Value>()
        .map_err(|err| format!("invalid plugin.toml {}: {err}", manifest_path.display()))?;
    let table = value
        .as_table_mut()
        .ok_or_else(|| format!("plugin.toml {} must be a table", manifest_path.display()))?;
    table.insert(
        "trust".to_string(),
        toml::Value::String(trust.as_str().to_string()),
    );
    table.remove("enabled");
    std::fs::write(
        &manifest_path,
        toml::to_string_pretty(&value)
            .map_err(|err| format!("failed to render {}: {err}", manifest_path.display()))?,
    )
    .map_err(|err| format!("failed to write {}: {err}", manifest_path.display()))?;
    Ok(manifest_path)
}

pub fn discover_plugin_mcp_servers(project_root: &Path) -> PluginMcpDiscovery {
    let mut discovery = PluginMcpDiscovery::default();
    for plugin in PluginRegistry::discover(project_root).enabled_plugins() {
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
    let mut discovery = PluginCommandDiscovery::default();
    for path in PluginRegistry::discover(project_root).enabled_command_paths() {
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

    fn write_manifest(dir: &Path, plugin: &str, manifest: &str) {
        let plugin_dir = dir.join(plugin);
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("plugin.toml"), manifest).unwrap();
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
        assert!(registry.diagnostics().is_empty());
        assert_eq!(
            registry.get("alpha").unwrap().trust,
            PluginTrustState::Disabled
        );
        assert_eq!(
            registry.get("zeta").unwrap().trust,
            PluginTrustState::Enabled
        );
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
        assert!(messages
            .iter()
            .any(|message| *message == "missing plugin.toml"));
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

        let registry = PluginRegistry::discover_dir(dir.path());
        let enabled = registry
            .enabled_plugins()
            .map(|plugin| plugin.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(enabled, vec!["enabled"]);
    }

    #[test]
    fn set_plugin_trust_updates_manifest() {
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

        let manifest = set_plugin_trust(dir.path(), "demo", PluginTrustState::Enabled).unwrap();
        let updated = std::fs::read_to_string(manifest).unwrap();
        let registry = PluginRegistry::discover(dir.path());

        assert!(updated.contains("trust = \"enabled\""));
        assert!(!updated.contains("enabled = false"));
        assert_eq!(
            registry.get("demo").unwrap().trust,
            PluginTrustState::Enabled
        );
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

        let discovery = discover_plugin_mcp_servers(dir.path());

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

        let discovery = discover_plugin_mcp_servers(dir.path());

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

        let discovery = discover_plugin_commands(dir.path());

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

        let discovery = discover_plugin_commands(dir.path());

        assert!(discovery.commands.is_empty());
        assert!(discovery.diagnostics.is_empty());
    }
}
