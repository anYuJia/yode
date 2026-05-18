use std::path::PathBuf;

use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use yode_core::plugins::{
    set_plugin_trust, Plugin, PluginContributions, PluginDiagnostic, PluginRegistry,
    PluginTrustState,
};

pub struct PluginCommand {
    meta: CommandMeta,
}

impl PluginCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "plugin",
                description: "List, inspect, enable, or disable local plugins",
                aliases: &["plugins"],
                args: vec![ArgDef {
                    name: "action".to_string(),
                    required: false,
                    hint: "list | inspect <name> | enable <name> | disable <name>".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "list".to_string(),
                        "inspect".to_string(),
                        "enable".to_string(),
                        "disable".to_string(),
                    ]),
                }],
                category: CommandCategory::Tools,
                hidden: false,
            },
        }
    }
}

impl Command for PluginCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let project_root = PathBuf::from(&ctx.session.working_dir);
        let parts = args.split_whitespace().collect::<Vec<_>>();

        match parts.as_slice() {
            [] | ["list"] => {
                let registry = PluginRegistry::discover(&project_root);
                Ok(CommandOutput::Message(render_plugin_list(&registry)))
            }
            ["inspect", name] => {
                let registry = PluginRegistry::discover(&project_root);
                registry
                    .get(name)
                    .map(|plugin| render_plugin_detail(plugin, registry.diagnostics()))
                    .map(CommandOutput::Message)
                    .ok_or_else(|| format!("Plugin '{}' not found.", name))
            }
            ["enable", name] => update_plugin_trust(&project_root, name, PluginTrustState::Enabled),
            ["disable", name] => {
                update_plugin_trust(&project_root, name, PluginTrustState::Disabled)
            }
            _ => Err("Usage: /plugin [list|inspect <name>|enable <name>|disable <name>]".into()),
        }
    }
}

fn update_plugin_trust(
    project_root: &std::path::Path,
    name: &str,
    trust: PluginTrustState,
) -> CommandResult {
    let manifest = set_plugin_trust(project_root, name, trust)?;
    Ok(CommandOutput::Message(format!(
        "Plugin '{}' is now {}. Manifest: {}",
        name,
        trust.as_str(),
        manifest.display()
    )))
}

fn render_plugin_list(registry: &PluginRegistry) -> String {
    let plugins = registry.plugins();
    let diagnostics = registry.diagnostics();
    if plugins.is_empty() && diagnostics.is_empty() {
        return "No plugins found. Add plugin.toml files under .yode/plugins/<name>/.".to_string();
    }

    let mut lines = vec![format!("Plugins ({}):", plugins.len())];
    for plugin in plugins {
        lines.push(format!(
            "  - {} [{}] {}{}",
            plugin.name,
            plugin.trust.as_str(),
            contribution_summary(&plugin.contributions),
            plugin
                .description
                .as_ref()
                .map(|description| format!(" - {}", description))
                .unwrap_or_default()
        ));
    }

    if !diagnostics.is_empty() {
        lines.push(format!("Diagnostics ({}):", diagnostics.len()));
        for diagnostic in diagnostics {
            lines.push(format!(
                "  - {}: {}",
                diagnostic.plugin_dir.display(),
                diagnostic.message
            ));
        }
    }

    lines.push("Use `/plugin inspect <name>` for contribution paths.".to_string());
    lines.join("\n")
}

fn render_plugin_detail(plugin: &Plugin, diagnostics: &[PluginDiagnostic]) -> String {
    let mut lines = vec![
        format!("Plugin: {}", plugin.name),
        format!("Trust: {}", plugin.trust.as_str()),
        format!(
            "Description: {}",
            plugin.description.as_deref().unwrap_or("(none)")
        ),
        format!("Root: {}", plugin.root.display()),
        format!("Manifest: {}", plugin.manifest_path.display()),
        format!(
            "Contributions: {}",
            contribution_summary(&plugin.contributions)
        ),
    ];
    append_paths(&mut lines, "Skills", &plugin.contributions.skills);
    append_paths(&mut lines, "Workflows", &plugin.contributions.workflows);
    append_paths(&mut lines, "Hooks", &plugin.contributions.hooks);
    append_paths(&mut lines, "Commands", &plugin.contributions.commands);
    if !plugin.contributions.mcp_servers.is_empty() {
        lines.push(format!(
            "MCP servers: {}",
            plugin.contributions.mcp_servers.join(", ")
        ));
    }

    let plugin_diagnostics = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.plugin_dir == plugin.root)
        .collect::<Vec<_>>();
    if !plugin_diagnostics.is_empty() {
        lines.push("Diagnostics:".to_string());
        for diagnostic in plugin_diagnostics {
            lines.push(format!("  - {}", diagnostic.message));
        }
    }

    lines.join("\n")
}

fn append_paths(lines: &mut Vec<String>, label: &str, paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }
    lines.push(format!("{}:", label));
    for path in paths {
        lines.push(format!("  - {}", path.display()));
    }
}

fn contribution_summary(contributions: &PluginContributions) -> String {
    format!(
        "skills={} workflows={} hooks={} commands={} mcp={}",
        contributions.skills.len(),
        contributions.workflows.len(),
        contributions.hooks.len(),
        contributions.commands.len(),
        contributions.mcp_servers.len()
    )
}

#[cfg(test)]
mod tests {
    use super::{render_plugin_detail, render_plugin_list};
    use yode_core::plugins::{Plugin, PluginContributions, PluginRegistry, PluginTrustState};

    fn sample_plugin(trust: PluginTrustState) -> Plugin {
        Plugin {
            name: "demo".to_string(),
            description: Some("Demo plugin".to_string()),
            trust,
            root: std::path::PathBuf::from("/tmp/demo"),
            manifest_path: std::path::PathBuf::from("/tmp/demo/plugin.toml"),
            contributions: PluginContributions {
                skills: vec![std::path::PathBuf::from("/tmp/demo/skills/demo/SKILL.md")],
                workflows: vec![std::path::PathBuf::from("/tmp/demo/workflows/demo.json")],
                hooks: Vec::new(),
                commands: Vec::new(),
                mcp_servers: vec!["docs".to_string()],
            },
        }
    }

    #[test]
    fn render_plugin_detail_includes_contributions() {
        let plugin = sample_plugin(PluginTrustState::Enabled);
        let rendered = render_plugin_detail(&plugin, &[]);

        assert!(rendered.contains("Plugin: demo"));
        assert!(rendered.contains("Trust: enabled"));
        assert!(rendered.contains("Skills:"));
        assert!(rendered.contains("Workflows:"));
        assert!(rendered.contains("MCP servers: docs"));
    }

    #[test]
    fn render_plugin_list_includes_diagnostics() {
        let dir = std::env::temp_dir().join(format!("yode-plugin-list-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugin_dir = dir.join("demo");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            r#"
name = "demo"
trust = "disabled"
skills = ["skills/demo/SKILL.md"]
"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("broken")).unwrap();
        let registry = PluginRegistry::discover_dir(&dir);

        let rendered = render_plugin_list(&registry);

        assert!(rendered.contains("demo [disabled]"));
        assert!(rendered.contains("Diagnostics (1):"));
        assert!(rendered.contains("missing plugin.toml"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
