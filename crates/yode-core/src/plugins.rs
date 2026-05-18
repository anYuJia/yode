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

    pub fn discover_dir(plugins_dir: &Path) -> Self {
        discover_plugins(plugins_dir)
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDiagnostic {
    pub plugin_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub message: String,
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

fn parse_plugin_manifest(plugin_dir: &Path, manifest_path: &Path) -> Result<Plugin, String> {
    let content = std::fs::read_to_string(manifest_path)
        .map_err(|err| format!("failed to read plugin.toml: {err}"))?;
    let manifest: PluginManifest =
        toml::from_str(&content).map_err(|err| format!("invalid plugin.toml: {err}"))?;

    let name = manifest.name.trim();
    if name.is_empty() {
        return Err("plugin name is required".to_string());
    }

    let trust = manifest.trust.unwrap_or_else(|| match manifest.enabled {
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
}
