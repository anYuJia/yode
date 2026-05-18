use std::path::PathBuf;

use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct SkillsCommand {
    meta: CommandMeta,
}

impl SkillsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "skills",
                description: "List discovered skills and path-gated metadata",
                aliases: &["skill"],
                args: vec![ArgDef {
                    name: "view".to_string(),
                    required: false,
                    hint: "list | show <name> | active <path> | search <query>".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "list".to_string(),
                        "show".to_string(),
                        "active".to_string(),
                        "search".to_string(),
                    ]),
                }],
                category: CommandCategory::Tools,
                hidden: false,
            },
        }
    }
}

impl Command for SkillsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let project_root = PathBuf::from(&ctx.session.working_dir);
        let registry = yode_core::skills::SkillRegistry::discover(
            &yode_core::skills::SkillRegistry::default_paths(&project_root),
        );
        let parts = args.split_whitespace().collect::<Vec<_>>();

        match parts.as_slice() {
            [] | ["list"] => Ok(CommandOutput::Message(render_skill_list(&registry))),
            ["show", name] => registry
                .get(name)
                .map(render_skill_detail)
                .map(CommandOutput::Message)
                .ok_or_else(|| format!("Skill '{}' not found.", name)),
            ["active"] => {
                let paths = runtime_recent_paths(ctx);
                if paths.is_empty() {
                    return Ok(CommandOutput::Message(
                        "No recent file paths are available yet. Use `/skills active <path>` or read files in the current session first."
                            .to_string(),
                    ));
                }
                let active = registry.active_for_paths(paths.iter().map(|path| path.as_str()));
                Ok(CommandOutput::Message(render_active_skills(
                    "recent read paths",
                    &paths,
                    &active,
                )))
            }
            ["active", paths @ ..] => {
                let path_list = paths
                    .iter()
                    .map(|path| path.to_string())
                    .collect::<Vec<_>>();
                let active = registry.active_for_paths(paths.iter().copied());
                Ok(CommandOutput::Message(render_active_skills(
                    &path_list.join(", "),
                    &path_list,
                    &active,
                )))
            }
            ["search", query @ ..] => {
                let query = query.join(" ");
                Ok(CommandOutput::Message(render_search_results(
                    &query, &registry,
                )))
            }
            _ => Err(
                "Usage: /skills [list|show <name>|active <path> [... ]|search <query>]".to_string(),
            ),
        }
    }
}

fn render_skill_list(registry: &yode_core::skills::SkillRegistry) -> String {
    let skills = registry.list();
    if skills.is_empty() {
        return "No skills found. Add SKILL.md files under .yode/skills/ or ~/.yode/skills/."
            .to_string();
    }

    let mut lines = vec![format!("Discovered skills ({}):", skills.len())];
    for skill in skills {
        lines.push(format!(
            "  - {} — {}{}",
            skill.name,
            skill.description,
            render_metadata_suffix(skill)
        ));
    }
    lines.push("Use `/skills show <name>` for details.".to_string());
    lines.join("\n")
}

fn render_skill_detail(skill: &yode_core::skills::Skill) -> String {
    let mut lines = vec![
        format!("Skill: {}", skill.name),
        format!("Description: {}", empty_label(&skill.description)),
        format!("Source: {}", skill.source.display()),
        format!("Context: {}", context_label(skill.metadata.context)),
    ];
    if !skill.metadata.allowed_tools.is_empty() {
        lines.push(format!(
            "Allowed tools: {}",
            skill.metadata.allowed_tools.join(", ")
        ));
    }
    if !skill.metadata.paths.is_empty() {
        lines.push(format!("Path gates: {}", skill.metadata.paths.join(", ")));
    }
    if let Some(model) = &skill.metadata.model {
        lines.push(format!("Preferred model: {}", model));
    }
    if let Some(effort) = &skill.metadata.effort {
        lines.push(format!("Preferred effort: {}", effort));
    }
    lines.push(format!("Body: {} chars", skill.content.chars().count()));
    lines.join("\n")
}

fn render_active_skills(
    label: &str,
    paths: &[String],
    skills: &[&yode_core::skills::Skill],
) -> String {
    if skills.is_empty() {
        return format!("No path-gated skills match '{}'.", label);
    }
    let mut lines = vec![format!("Path-gated skills active for '{}':", label)];
    if !paths.is_empty() {
        lines.push(format!("  Paths: {}", paths.join(" | ")));
    }
    for skill in skills {
        lines.push(format!(
            "  - {} — {}{}",
            skill.name,
            skill.description,
            render_metadata_suffix(skill)
        ));
    }
    lines.join("\n")
}

fn render_search_results(query: &str, registry: &yode_core::skills::SkillRegistry) -> String {
    let results = registry.search(query);
    if results.is_empty() {
        return format!("No skills matched '{}'.", query);
    }

    let mut lines = vec![format!("Skill search results for '{}':", query)];
    for result in results.iter().take(10) {
        lines.push(format!(
            "  - {} — {} [score={} | {}]",
            result.skill.name,
            result.skill.description,
            result.score,
            result.reasons.join(", ")
        ));
    }
    lines.join("\n")
}

fn runtime_recent_paths(ctx: &mut CommandContext<'_>) -> Vec<String> {
    let runtime = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| engine.runtime_state());
    runtime
        .map(|state| state.read_file_history)
        .unwrap_or_default()
}

fn render_metadata_suffix(skill: &yode_core::skills::Skill) -> String {
    let mut tags = Vec::new();
    if !skill.metadata.paths.is_empty() {
        tags.push(format!("paths:{}", skill.metadata.paths.join(",")));
    }
    if skill.metadata.context == yode_core::skills::SkillContextMode::Fork {
        tags.push("context:fork".to_string());
    }
    if let Some(model) = &skill.metadata.model {
        tags.push(format!("model:{}", model));
    }
    if tags.is_empty() {
        String::new()
    } else {
        format!(" [{}]", tags.join(" | "))
    }
}

fn context_label(context: yode_core::skills::SkillContextMode) -> &'static str {
    match context {
        yode_core::skills::SkillContextMode::Inline => "inline",
        yode_core::skills::SkillContextMode::Fork => "fork",
    }
}

fn empty_label(value: &str) -> &str {
    if value.trim().is_empty() {
        "(none)"
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::{render_active_skills, render_search_results};
    use yode_core::skills::{Skill, SkillContextMode, SkillMetadata};

    #[test]
    fn active_skills_render_includes_recent_paths() {
        let skill = Skill {
            name: "rust".to_string(),
            description: "Rust guidance".to_string(),
            content: "Use cargo test.".to_string(),
            source: std::path::PathBuf::from("/tmp/SKILL.md"),
            metadata: SkillMetadata {
                allowed_tools: vec![],
                paths: vec!["crates/**".to_string()],
                trigger_examples: Vec::new(),
                context: SkillContextMode::Inline,
                model: None,
                effort: None,
            },
        };
        let paths = vec!["crates/yode-core/src/lib.rs".to_string()];

        let rendered = render_active_skills("recent read paths", &paths, &[&skill]);

        assert!(rendered.contains("recent read paths"));
        assert!(rendered.contains("crates/yode-core/src/lib.rs"));
        assert!(rendered.contains("rust"));
    }

    #[test]
    fn active_skills_render_includes_multiple_paths() {
        let skill = Skill {
            name: "docs".to_string(),
            description: "Docs guidance".to_string(),
            content: "Keep docs concise.".to_string(),
            source: std::path::PathBuf::from("/tmp/SKILL.md"),
            metadata: SkillMetadata {
                allowed_tools: vec![],
                paths: vec!["docs/**".to_string()],
                trigger_examples: Vec::new(),
                context: SkillContextMode::Inline,
                model: None,
                effort: None,
            },
        };
        let paths = vec!["docs/guide.md".to_string(), "src/main.rs".to_string()];

        let rendered = render_active_skills("docs/guide.md, src/main.rs", &paths, &[&skill]);

        assert!(rendered.contains("docs/guide.md | src/main.rs"));
        assert!(rendered.contains("docs"));
    }

    #[test]
    fn search_results_render_scores_and_reasons() {
        let dir = std::env::temp_dir().join(format!("yode-skills-search-{}", uuid::Uuid::new_v4()));
        let skill_path = dir.join("rust").join("SKILL.md");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(skill_path.parent().unwrap()).unwrap();
        std::fs::write(
            &skill_path,
            "---\nname: rust\ndescription: Rust guidance\npaths:\n  - crates/**\ntrigger-examples:\n  - review rust changes\n---\nUse cargo test.\n",
        )
        .unwrap();
        let registry = yode_core::skills::SkillRegistry::discover(&[dir.clone()]);

        let rendered = render_search_results("rust crates", &registry);

        assert!(rendered.contains("score="));
        assert!(rendered.contains("name exact"));
        assert!(rendered.contains("paths"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
