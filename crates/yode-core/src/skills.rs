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
    pub trigger_examples: Vec<String>,
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
    #[serde(
        default,
        alias = "trigger_examples",
        alias = "trigger-examples",
        alias = "triggers",
        alias = "examples"
    )]
    trigger_examples: Vec<String>,
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
            trigger_examples: normalized_list(&self.trigger_examples),
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
    diagnostics: Vec<SkillDiagnostic>,
}

#[derive(Debug, Clone)]
pub struct SkillSearchResult<'a> {
    pub skill: &'a Skill,
    pub score: u32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ActiveSkillMatch<'a> {
    pub skill: &'a Skill,
    pub matched_paths: Vec<String>,
    pub matched_patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDiagnostic {
    pub path: PathBuf,
    pub message: String,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Discover skills from multiple directories.
    pub fn discover(paths: &[PathBuf]) -> Self {
        let mut registry = Self::new();
        for path in paths {
            registry.discover_dir(path);
        }
        registry
    }

    /// Discover skills from multiple directories without blocking the async runtime.
    pub async fn discover_async(paths: &[PathBuf]) -> Self {
        let mut registry = Self::new();
        for path in paths {
            registry.discover_dir_async(path).await;
        }
        registry
    }

    /// Discover skills in a single directory.
    fn discover_dir(&mut self, dir: &Path) {
        if !dir.is_dir() {
            for skill_path in candidate_skill_files(dir) {
                self.parse_and_push_skill(&skill_path);
            }
            if !dir.exists() && looks_like_skill_file_reference(dir) {
                self.diagnostics.push(SkillDiagnostic {
                    path: dir.to_path_buf(),
                    message: "referenced skill file is missing".to_string(),
                });
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
                self.parse_and_push_skill(&skill_path);
            }
        }
    }

    /// Discover skills in a single directory without blocking the async runtime.
    async fn discover_dir_async(&mut self, dir: &Path) {
        if tokio::fs::metadata(dir)
            .await
            .map(|metadata| !metadata.is_dir())
            .unwrap_or(true)
        {
            for skill_path in candidate_skill_files(dir) {
                self.parse_and_push_skill_async(&skill_path).await;
            }
            if tokio::fs::metadata(dir).await.is_err() && looks_like_skill_file_reference(dir) {
                self.diagnostics.push(SkillDiagnostic {
                    path: dir.to_path_buf(),
                    message: "referenced skill file is missing".to_string(),
                });
            }
            return;
        }

        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(entries) => entries,
            Err(_) => return,
        };
        let mut paths = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            paths.push(entry.path());
        }
        paths.sort();

        for path in paths {
            for skill_path in candidate_skill_files(&path) {
                self.parse_and_push_skill_async(&skill_path).await;
            }
        }
    }

    fn parse_and_push_skill(&mut self, skill_path: &Path) {
        match Self::parse_skill_file(skill_path) {
            Ok(Some(skill)) => self.push_unique_skill(skill, skill_path),
            Ok(None) => {}
            Err(message) => self.push_diagnostic(skill_path, message),
        }
    }

    async fn parse_and_push_skill_async(&mut self, skill_path: &Path) {
        match Self::parse_skill_file_async(skill_path).await {
            Ok(Some(skill)) => self.push_unique_skill(skill, skill_path),
            Ok(None) => {}
            Err(message) => self.push_diagnostic(skill_path, message),
        }
    }

    fn push_unique_skill(&mut self, skill: Skill, skill_path: &Path) {
        if !self.skills.iter().any(|s| s.name == skill.name) {
            tracing::debug!(
                name = %skill.name,
                path = %skill_path.display(),
                "Discovered skill"
            );
            self.skills.push(skill);
        }
    }

    fn push_diagnostic(&mut self, path: &Path, message: impl Into<String>) {
        self.diagnostics.push(SkillDiagnostic {
            path: path.to_path_buf(),
            message: message.into(),
        });
    }

    /// Parse a single SKILL.md file into a Skill.
    fn parse_skill_file(path: &Path) -> Result<Option<Skill>, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read skill file: {error}"))?;
        Self::parse_skill_content(path, content)
    }

    /// Parse a single SKILL.md file into a Skill without blocking the async runtime.
    async fn parse_skill_file_async(path: &Path) -> Result<Option<Skill>, String> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|error| format!("failed to read skill file: {error}"))?;
        Self::parse_skill_content(path, content)
    }

    fn parse_skill_content(path: &Path, content: String) -> Result<Option<Skill>, String> {
        // Parse YAML frontmatter between --- delimiters
        if !content.starts_with("---") {
            return Err("missing YAML frontmatter opener".to_string());
        }

        let rest = &content[3..];
        let end = rest
            .find("---")
            .ok_or_else(|| "missing YAML frontmatter closer".to_string())?;
        let frontmatter_str = &rest[..end];
        let body = rest[end + 3..].trim_start().to_string();

        let fm: SkillFrontmatter = serde_yaml_ng::from_str(frontmatter_str)
            .map_err(|error| format!("invalid YAML frontmatter: {error}"))?;

        let metadata = fm.metadata();

        Ok(Some(Skill {
            name: fm.name,
            description: fm.description,
            content: body,
            source: path.to_path_buf(),
            metadata,
        }))
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.name == name)
    }

    /// List all skills.
    pub fn list(&self) -> &[Skill] {
        &self.skills
    }

    pub fn diagnostics(&self) -> &[SkillDiagnostic] {
        &self.diagnostics
    }

    pub fn active_for_paths<I, P>(&self, changed_paths: I) -> Vec<&Skill>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.active_for_paths_with_reasons(changed_paths)
            .into_iter()
            .map(|active| active.skill)
            .collect()
    }

    pub fn active_for_paths_with_reasons<I, P>(&self, changed_paths: I) -> Vec<ActiveSkillMatch<'_>>
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
            .filter_map(|skill| active_skill_match(skill, &normalized_paths))
            .collect()
    }

    pub fn search(&self, query: &str) -> Vec<SkillSearchResult<'_>> {
        let tokens = search_tokens(query);
        if tokens.is_empty() {
            return self
                .skills
                .iter()
                .map(|skill| SkillSearchResult {
                    skill,
                    score: 0,
                    reasons: vec!["listed".to_string()],
                })
                .collect();
        }

        let mut results = self
            .skills
            .iter()
            .filter_map(|skill| score_skill_search(skill, &tokens))
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.skill.name.cmp(&right.skill.name))
                .then_with(|| left.skill.source.cmp(&right.skill.source))
        });
        results
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

    /// Get default discovery paths based on working directory without blocking the async runtime.
    pub async fn default_paths_async(working_dir: &Path) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        paths.push(working_dir.join(".yode").join("skills"));

        paths.extend(
            crate::plugins::PluginRegistry::discover_async(working_dir)
                .await
                .enabled_skill_paths(),
        );

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

fn active_skill_match<'a>(
    skill: &'a Skill,
    normalized_paths: &[String],
) -> Option<ActiveSkillMatch<'a>> {
    if skill.metadata.paths.is_empty() || normalized_paths.is_empty() {
        return None;
    }

    let mut matched_paths = Vec::new();
    let mut matched_patterns = Vec::new();
    for pattern in &skill.metadata.paths {
        for path in normalized_paths {
            if path_matches_skill_pattern(path, pattern) {
                push_unique_string(&mut matched_patterns, pattern);
                push_unique_string(&mut matched_paths, path);
            }
        }
    }

    (!matched_paths.is_empty()).then_some(ActiveSkillMatch {
        skill,
        matched_paths,
        matched_patterns,
    })
}

fn search_tokens(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect()
}

fn score_skill_search<'a>(skill: &'a Skill, tokens: &[String]) -> Option<SkillSearchResult<'a>> {
    let name = skill.name.to_ascii_lowercase();
    let description = skill.description.to_ascii_lowercase();
    let paths = skill
        .metadata
        .paths
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let triggers = skill
        .metadata
        .trigger_examples
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut score = 0;
    let mut reasons = Vec::new();

    for token in tokens {
        if name == *token {
            score += 100;
            push_unique_reason(&mut reasons, "name exact");
        } else if name.contains(token) {
            score += 80;
            push_unique_reason(&mut reasons, "name");
        }
        if description.contains(token) {
            score += 50;
            push_unique_reason(&mut reasons, "description");
        }
        if paths.iter().any(|path| path.contains(token)) {
            score += 40;
            push_unique_reason(&mut reasons, "paths");
        }
        if triggers.iter().any(|trigger| trigger.contains(token)) {
            score += 60;
            push_unique_reason(&mut reasons, "triggers");
        }
    }

    (score > 0).then_some(SkillSearchResult {
        skill,
        score,
        reasons,
    })
}

fn push_unique_reason(reasons: &mut Vec<String>, reason: &str) {
    if !reasons.iter().any(|existing| existing == reason) {
        reasons.push(reason.to_string());
    }
}

fn push_unique_string(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
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

fn looks_like_skill_file_reference(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    file_name.eq_ignore_ascii_case("SKILL.md")
        || path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
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
        assert!(skill.metadata.trigger_examples.is_empty());
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

        let active = registry.active_for_paths_with_reasons(["crates/yode-core/src/lib.rs"]);
        assert_eq!(active[0].skill.name, "rust");
        assert_eq!(
            active[0].matched_paths,
            vec!["crates/yode-core/src/lib.rs".to_string()]
        );
        assert_eq!(
            active[0].matched_patterns,
            vec!["crates/yode-core/**".to_string(), "*.rs".to_string()]
        );
    }

    #[test]
    fn search_ranks_name_description_paths_and_triggers_deterministically() {
        let dir = tempfile::tempdir().unwrap();
        let rust_path = dir.path().join("rust").join("SKILL.md");
        std::fs::create_dir_all(rust_path.parent().unwrap()).unwrap();
        std::fs::write(
            &rust_path,
            "---\nname: rust\ndescription: Rust crate review\npaths:\n  - crates/**/*.rs\ntrigger-examples:\n  - review unsafe Rust changes\n---\nUse cargo test.\n",
        )
        .unwrap();
        let docs_path = dir.path().join("docs").join("SKILL.md");
        std::fs::create_dir_all(docs_path.parent().unwrap()).unwrap();
        std::fs::write(
            &docs_path,
            "---\nname: docs\ndescription: Documentation guidance\npaths:\n  - crates/docs/**\ntrigger-examples:\n  - update markdown guide\n---\nKeep docs concise.\n",
        )
        .unwrap();

        let registry = SkillRegistry::discover(&[dir.path().to_path_buf()]);
        let results = registry.search("rust crates");

        assert_eq!(results[0].skill.name, "rust");
        assert!(results[0].score > results[1].score);
        assert!(results[0].reasons.contains(&"name exact".to_string()));
        assert!(results[0].reasons.contains(&"paths".to_string()));
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
    fn missing_referenced_skill_files_emit_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join(".yode").join("plugins").join("review");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            r#"
name = "review"
trust = "enabled"
skills = ["skills/missing/SKILL.md"]
"#,
        )
        .unwrap();

        let registry = SkillRegistry::discover(&SkillRegistry::default_paths(dir.path()));

        assert!(registry.get("plugin-review").is_none());
        assert_eq!(registry.diagnostics().len(), 1);
        assert!(registry.diagnostics()[0]
            .path
            .display()
            .to_string()
            .contains("skills/missing/SKILL.md"));
        assert!(registry.diagnostics()[0].message.contains("missing"));
    }

    #[test]
    fn invalid_skill_files_emit_diagnostics_and_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let missing_opener = dir.path().join("missing-opener.md");
        let missing_closer = dir.path().join("missing-closer.md");
        let invalid_yaml = dir.path().join("invalid-yaml.md");
        let valid = dir.path().join("valid.md");

        std::fs::write(&missing_opener, "name: missing-opener\nbody").unwrap();
        std::fs::write(&missing_closer, "---\nname: missing-closer\nbody").unwrap();
        std::fs::write(&invalid_yaml, "---\nname: [broken\n---\nbody").unwrap();
        write_skill(&valid, "valid", "Valid skill", "valid body");

        let registry = SkillRegistry::discover(&[dir.path().to_path_buf()]);

        assert_eq!(registry.list().len(), 1);
        assert!(registry.get("valid").is_some());
        assert_eq!(registry.diagnostics().len(), 3);

        let messages = registry
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();
        assert!(messages
            .iter()
            .any(|message| message.contains("missing YAML frontmatter opener")));
        assert!(messages
            .iter()
            .any(|message| message.contains("missing YAML frontmatter closer")));
        assert!(messages
            .iter()
            .any(|message| message.contains("invalid YAML frontmatter")));
    }

    #[tokio::test]
    async fn invalid_skill_files_emit_async_diagnostics_and_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let invalid_yaml = dir.path().join("invalid-yaml.md");
        let valid = dir.path().join("valid.md");

        std::fs::write(&invalid_yaml, "---\nname: [broken\n---\nbody").unwrap();
        write_skill(&valid, "valid", "Valid skill", "valid body");

        let registry = SkillRegistry::discover_async(&[dir.path().to_path_buf()]).await;

        assert_eq!(registry.list().len(), 1);
        assert!(registry.get("valid").is_some());
        assert_eq!(registry.diagnostics().len(), 1);
        assert!(registry.diagnostics()[0]
            .message
            .contains("invalid YAML frontmatter"));
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
