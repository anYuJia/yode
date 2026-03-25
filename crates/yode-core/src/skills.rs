use std::path::{Path, PathBuf};

use serde::Deserialize;

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
}

/// SKILL.md frontmatter.
#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    #[serde(default)]
    description: String,
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
            return;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name.ends_with(".md") || name.ends_with(".MD") {
                    if let Some(skill) = Self::parse_skill_file(&path) {
                        // Avoid duplicates (project-level overrides global)
                        if !self.skills.iter().any(|s| s.name == skill.name) {
                            tracing::debug!(
                                name = %skill.name,
                                path = %path.display(),
                                "Discovered skill"
                            );
                            self.skills.push(skill);
                        }
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

        Some(Skill {
            name: fm.name,
            description: fm.description,
            content: body,
            source: path.to_path_buf(),
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

    /// Get default discovery paths based on working directory.
    pub fn default_paths(working_dir: &Path) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Project-level skills (highest priority)
        let project_skills = working_dir.join(".yode").join("skills");
        paths.push(project_skills);

        // Global skills
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".yode").join("skills"));
        }

        paths
    }
}
