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
}
