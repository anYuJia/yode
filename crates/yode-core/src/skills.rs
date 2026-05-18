use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A parsed skill from a SKILL.md file.
#[derive(Debug, Clone)]
pub struct Skill {
    /// Unique name of the skill (from frontmatter).
    pub name: String,
    /// Brief description of the skill.
    pub description: String,
    /// Full markdown content (after frontmatter).
    pub content: String,
    /// Path to the source file.
    pub source: PathBuf,
    /// Optional Claude Code-style skill metadata from frontmatter.
    pub metadata: SkillMetadata,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SkillMetadata {
    pub allowed_tools: Vec<String>,
    pub paths: Vec<String>,
    pub context: SkillContextMode,
    pub model: Option<String>,
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillContextMode {
    #[default]
    Inline,
    Fork,
}

/// SKILL.md frontmatter.
#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default, alias = "allowed_tools", rename = "allowed-tools")]
    allowed_tools: Vec<String>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    context: SkillContextModeFrontmatter,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    effort: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum SkillContextModeFrontmatter {
    #[default]
    Inline,
    Fork,
}

impl From<SkillContextModeFrontmatter> for SkillContextMode {
    fn from(value: SkillContextModeFrontmatter) -> Self {
        match value {
            SkillContextModeFrontmatter::Inline => Self::Inline,
            SkillContextModeFrontmatter::Fork => Self::Fork,
        }
    }
}

impl SkillFrontmatter {
    fn metadata(&self) -> SkillMetadata {
        SkillMetadata {
            allowed_tools: normalized_list(&self.allowed_tools),
            paths: normalized_list(&self.paths),
            context: self.context.into(),
            model: normalized_option(self.model.as_deref()),
            effort: normalized_option(self.effort.as_deref()),
        }
    }
}

/// Registry of discovered skills.
#[derive(Debug, Default)]
pub struct SkillRegistry {
    skills: Vec<Skill>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self { skills: Vec::new() }
    }

    /// Discover skills from multiple directories.
    pub fn discover(paths: &[PathBuf]) -> Self {
        let mut registry = Self::new();
        for path in paths {
            registry.discover_dir(path);
        }
        registry
    }

    /// Discover skills in a single directory.
    fn discover_dir(&mut self, dir: &Path) {
        if !dir.is_dir() {
            for skill_path in candidate_skill_files(dir) {
                if let Some(skill) = Self::parse_skill_file(&skill_path) {
                    if !self.skills.iter().any(|s| s.name == skill.name) {
                        tracing::debug!(
                            name = %skill.name,
                            path = %skill_path.display(),
                            "Discovered skill"
                        );
                        self.skills.push(skill);
                    }
                }
            }
            return;
        }

        let mut entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        }
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
        entries.sort();

        for path in entries {
            for skill_path in candidate_skill_files(&path) {
                if let Some(skill) = Self::parse_skill_file(&skill_path) {
                    // Avoid duplicates (project-level overrides global).
                    if !self.skills.iter().any(|s| s.name == skill.name) {
                        tracing::debug!(
                            name = %skill.name,
                            path = %skill_path.display(),
                            "Discovered skill"
                        );
                        self.skills.push(skill);
                    }
                }
            }
        }
    }

    /// Parse a single SKILL.md file into a Skill.
    fn parse_skill_file(path: &Path) -> Option<Skill> {
        let content = std::fs::read_to_string(path).ok()?;

        // Parse YAML frontmatter between --- delimiters
        if !content.starts_with("---") {
            return None;
        }

        let rest = &content[3..];
        let end = rest.find("---")?;
        let frontmatter_str = &rest[..end];
        let body = rest[end + 3..].trim_start().to_string();

        let fm: SkillFrontmatter = serde_yaml_ng::from_str(frontmatter_str).ok()?;

        let metadata = fm.metadata();

        Some(Skill {
            name: fm.name,
            description: fm.description,
            content: body,
            source: path.to_path_buf(),
            metadata,
        })
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.name == name)
    }

    /// List all skills.
    pub fn list(&self) -> &[Skill] {
        &self.skills
    }

    pub fn active_for_paths<I, P>(&self, changed_paths: I) -> Vec<&Skill>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let normalized_paths = changed_paths
            .into_iter()
            .map(|path| path.as_ref().to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>();

        self.skills
            .iter()
            .filter(|skill| {
                !skill.metadata.paths.is_empty()
                    && skill.metadata.paths.iter().any(|pattern| {
                        normalized_paths
                            .iter()
                            .any(|path| path_matches_skill_pattern(path, pattern))
                    })
            })
            .collect()
    }

    /// Get default discovery paths based on working directory.
    pub fn default_paths(working_dir: &Path) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Project-level skills (highest priority)
        let project_skills = working_dir.join(".yode").join("skills");
        paths.push(project_skills);

        paths.extend(crate::plugins::PluginRegistry::discover(working_dir).enabled_skill_paths());

        // Global skills
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".yode").join("skills"));
        }

        paths
    }
}

fn normalized_list(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn normalized_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn path_matches_skill_pattern(path: &str, pattern: &str) -> bool {
    let pattern = pattern.trim().replace('\\', "/");
    if pattern.is_empty() {
        return false;
    }

    if pattern == "*" || pattern == "**" || pattern == "**/*" {
        return true;
    }

    if let Some(suffix) = pattern.strip_prefix("**/") {
        return path.ends_with(suffix);
    }

    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path == prefix || path.starts_with(&format!("{}/", prefix));
    }

    if let Some(suffix) = pattern.strip_prefix('*') {
        return path.ends_with(suffix);
    }

    if let Some(prefix) = pattern.strip_suffix('*') {
        return path.starts_with(prefix);
    }

    path == pattern || path.starts_with(&format!("{}/", pattern))
}

fn candidate_skill_files(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.ends_with(".md") || name.ends_with(".MD") {
            return vec![path.to_path_buf()];
        }
        return Vec::new();
    }

    if path.is_dir() {
        let skill_md = path.join("SKILL.md");
        if skill_md.is_file() {
            return vec![skill_md];
        }
        let skill_md_lower = path.join("skill.md");
        if skill_md_lower.is_file() {
            return vec![skill_md_lower];
        }
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(path: &Path, name: &str, description: &str, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            format!(
                "---\nname: {}\ndescription: {}\n---\n{}",
                name, description, body
            ),
        )
        .unwrap();
    }

    #[test]
    fn discovers_markdown_and_directory_skills_deterministically() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(&dir.path().join("zeta.md"), "zeta", "Z skill", "zeta body");
        write_skill(
            &dir.path().join("alpha").join("SKILL.md"),
            "alpha",
            "A skill",
            "alpha body",
        );

        let registry = SkillRegistry::discover(&[dir.path().to_path_buf()]);
        let names = registry
            .list()
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["alpha", "zeta"]);
        assert_eq!(registry.get("alpha").unwrap().content, "alpha body");
    }

    #[test]
    fn earlier_paths_override_duplicate_skill_names() {
        let project = tempfile::tempdir().unwrap();
        let global = tempfile::tempdir().unwrap();
        write_skill(
            &project.path().join("review").join("SKILL.md"),
            "review",
            "project",
            "project body",
        );
        write_skill(
            &global.path().join("review.md"),
            "review",
            "global",
            "global body",
        );

        let registry =
            SkillRegistry::discover(&[project.path().to_path_buf(), global.path().to_path_buf()]);

        assert_eq!(registry.list().len(), 1);
        assert_eq!(registry.get("review").unwrap().description, "project");
        assert_eq!(registry.get("review").unwrap().content, "project body");
    }

    #[test]
    fn parses_claude_style_skill_frontmatter_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rust").join("SKILL.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "---\nname: rust\ndescription: Rust guidance\nallowed-tools:\n  - read_file\n  - bash\npaths:\n  - crates/**/*.rs\n  - Cargo.toml\ncontext: fork\nmodel: claude-sonnet-4-5\neffort: high\n---\nUse cargo test.\n",
        )
        .unwrap();

        let registry = SkillRegistry::discover(&[dir.path().to_path_buf()]);
        let skill = registry.get("rust").unwrap();

        assert_eq!(
            skill.metadata.allowed_tools,
            vec!["read_file".to_string(), "bash".to_string()]
        );
        assert_eq!(
            skill.metadata.paths,
            vec!["crates/**/*.rs".to_string(), "Cargo.toml".to_string()]
        );
        assert_eq!(skill.metadata.context, SkillContextMode::Fork);
        assert_eq!(skill.metadata.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(skill.metadata.effort.as_deref(), Some("high"));
    }

    #[test]
    fn active_for_paths_uses_path_gated_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rust").join("SKILL.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "---\nname: rust\ndescription: Rust guidance\npaths:\n  - crates/yode-core/**\n  - '*.rs'\n---\nUse cargo test.\n",
        )
        .unwrap();

        let registry = SkillRegistry::discover(&[dir.path().to_path_buf()]);
        let active = registry.active_for_paths(["crates/yode-core/src/lib.rs"]);

        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "rust");
    }

    #[test]
    fn default_paths_include_enabled_plugin_skills() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join(".yode").join("plugins").join("review");
        let skill_path = plugin_dir.join("skills").join("review").join("SKILL.md");
        write_skill(&skill_path, "plugin-review", "Plugin review", "review body");
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            r#"
name = "review"
trust = "enabled"
skills = ["skills/review/SKILL.md"]
"#,
        )
        .unwrap();

        let registry = SkillRegistry::discover(&SkillRegistry::default_paths(dir.path()));

        assert!(registry.get("plugin-review").is_some());
    }

    #[test]
    fn disabled_plugin_skills_are_not_discovered() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join(".yode").join("plugins").join("review");
        let skill_path = plugin_dir.join("skills").join("review").join("SKILL.md");
        write_skill(&skill_path, "plugin-review", "Plugin review", "review body");
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            r#"
name = "review"
trust = "disabled"
skills = ["skills/review/SKILL.md"]
"#,
        )
        .unwrap();

        let registry = SkillRegistry::discover(&SkillRegistry::default_paths(dir.path()));

        assert!(registry.get("plugin-review").is_none());
    }
}
