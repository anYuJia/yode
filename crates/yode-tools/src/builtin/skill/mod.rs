use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::tool::{SubAgentOptions, Tool, ToolCapabilities, ToolContext, ToolResult};

pub mod discover;

/// A lightweight store for skill content, populated at startup.
pub struct SkillStore {
    skills: Vec<SkillEntry>,
}

#[derive(Clone)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub content: String,
    pub allowed_tools: Vec<String>,
    pub paths: Vec<String>,
    pub context: SkillContextMode,
    pub model: Option<String>,
    pub effort: Option<String>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SkillContextMode {
    #[default]
    Inline,
    Fork,
}

impl SkillContextMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Fork => "fork",
        }
    }
}

impl SkillStore {
    pub fn new() -> Self {
        Self { skills: Vec::new() }
    }

    pub fn add(&mut self, name: String, description: String, content: String) {
        self.add_entry(SkillEntry {
            name,
            description,
            content,
            allowed_tools: Vec::new(),
            paths: Vec::new(),
            context: SkillContextMode::Inline,
            model: None,
            effort: None,
        });
    }

    pub fn add_entry(&mut self, entry: SkillEntry) {
        self.skills.push(SkillEntry {
            name: entry.name,
            description: entry.description,
            content: entry.content,
            allowed_tools: entry.allowed_tools,
            paths: entry.paths,
            context: entry.context,
            model: entry.model,
            effort: entry.effort,
        });
    }

    pub fn get(&self, name: &str) -> Option<&SkillEntry> {
        self.skills.iter().find(|s| s.name == name)
    }

    pub fn list(&self) -> &[SkillEntry] {
        &self.skills
    }
}

impl Default for SkillStore {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SkillTool {
    pub store: Arc<Mutex<SkillStore>>,
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "skill"
    }

    fn user_facing_name(&self) -> &str {
        "Skills"
    }

    fn activity_description(&self, params: &Value) -> String {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("get");
        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if action == "list" {
            "Listing skills".to_string()
        } else {
            format!("Loading skill: {}", name)
        }
    }

    fn description(&self) -> &str {
        "Load a skill by name and return its content. Skills are instructions defined in .yode/skills/ or ~/.yode/skills/ as markdown files with YAML frontmatter."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the skill to load"
                },
                "action": {
                    "type": "string",
                    "enum": ["get", "list", "run"],
                    "description": "Action to perform. 'get' loads a skill, 'list' lists all available skills, 'run' executes the skill workflow. Default: 'get'"
                },
                "prompt": {
                    "type": "string",
                    "description": "Task prompt to pass when action='run'. For fork skills this is sent to a sub-agent with the skill content."
                }
            },
            "required": ["name"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("get");

        match action {
            "list" => {
                let store = self.store.lock().await;
                let skills = store.list();
                if skills.is_empty() {
                    let metadata = json!({ "action": "list", "count": 0 });
                    return Ok(ToolResult::success_with_metadata(
                        "No skills found. Add .md files to .yode/skills/ or ~/.yode/skills/."
                            .to_string(),
                        metadata,
                    ));
                }
                let mut output = String::from("Available skills:\n");
                for skill in skills {
                    output.push_str(&format!(
                        "  /{} — {}{}\n",
                        skill.name,
                        skill.description,
                        render_skill_metadata_suffix(skill)
                    ));
                }
                let metadata = json!({
                    "action": "list",
                    "count": skills.len(),
                    "skills": skills.iter().map(skill_metadata_json).collect::<Vec<_>>()
                });
                Ok(ToolResult::success_with_metadata(output, metadata))
            }
            "run" => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;
                let prompt = params
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let skill = {
                    let store = self.store.lock().await;
                    store.get(name).cloned()
                };
                let Some(skill) = skill else {
                    let available = {
                        let store = self.store.lock().await;
                        store
                            .list()
                            .iter()
                            .map(|s| s.name.clone())
                            .collect::<Vec<_>>()
                    };
                    return Ok(ToolResult::error(format!(
                        "Skill '{}' not found. Available skills: {:?}",
                        name, available
                    )));
                };

                run_skill(skill, prompt, ctx).await
            }
            _ => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;

                let store = self.store.lock().await;
                match store.get(name) {
                    Some(skill) => {
                        let metadata = json!({
                            "action": "get",
                            "name": name,
                            "skill": skill_metadata_json(skill)
                        });
                        let mut content = String::new();
                        let metadata_lines = render_skill_metadata_block(skill);
                        if !metadata_lines.is_empty() {
                            content.push_str(&metadata_lines);
                            content.push_str("\n\n");
                        }
                        content.push_str(&skill.content);
                        Ok(ToolResult::success_with_metadata(content, metadata))
                    }
                    None => {
                        let available: Vec<&str> =
                            store.list().iter().map(|s| s.name.as_str()).collect();
                        Ok(ToolResult::error(format!(
                            "Skill '{}' not found. Available skills: {:?}",
                            name, available
                        )))
                    }
                }
            }
        }
    }
}

async fn run_skill(skill: SkillEntry, prompt: String, ctx: &ToolContext) -> Result<ToolResult> {
    match skill.context {
        SkillContextMode::Fork => {
            let Some(runner) = ctx.sub_agent_runner.as_ref() else {
                return Ok(ToolResult::error(format!(
                    "Skill '{}' requests context=fork, but no sub-agent runner is available in this context.",
                    skill.name
                )));
            };
            let run_prompt = render_skill_run_prompt(&skill, &prompt);
            let output = runner
                .run_sub_agent(
                    run_prompt,
                    SubAgentOptions {
                        description: format!("Run skill '{}'", skill.name),
                        subagent_type: Some("skill".to_string()),
                        model: skill.model.clone(),
                        run_in_background: false,
                        isolation: Some("in-process".to_string()),
                        cwd: ctx.working_dir.clone(),
                        allowed_tools: skill.allowed_tools.clone(),
                        team_id: None,
                        member_id: None,
                    },
                )
                .await?;
            Ok(ToolResult::success_with_metadata(
                output,
                json!({
                    "action": "run",
                    "mode": "fork",
                    "skill": skill_metadata_json(&skill),
                }),
            ))
        }
        SkillContextMode::Inline => {
            let mut content = String::new();
            let metadata_lines = render_skill_metadata_block(&skill);
            if !metadata_lines.is_empty() {
                content.push_str(&metadata_lines);
                content.push_str("\n\n");
            }
            content.push_str(&skill.content);
            if !prompt.is_empty() {
                content.push_str("\n\nTask prompt:\n");
                content.push_str(&prompt);
            }
            Ok(ToolResult::success_with_metadata(
                content,
                json!({
                    "action": "run",
                    "mode": "inline",
                    "skill": skill_metadata_json(&skill),
                }),
            ))
        }
    }
}

fn render_skill_run_prompt(skill: &SkillEntry, prompt: &str) -> String {
    let mut body = String::new();
    body.push_str("You are running a Yode skill in an isolated context.\n\n");
    body.push_str(&format!("Skill: {}\n", skill.name));
    body.push_str(&format!("Description: {}\n\n", skill.description));
    let metadata = render_skill_metadata_block(skill);
    if !metadata.is_empty() {
        body.push_str(&metadata);
        body.push_str("\n\n");
    }
    body.push_str("Skill instructions:\n");
    body.push_str(&skill.content);
    if !prompt.trim().is_empty() {
        body.push_str("\n\nUser task:\n");
        body.push_str(prompt.trim());
    }
    body
}

fn render_skill_metadata_suffix(skill: &SkillEntry) -> String {
    let mut tags = Vec::new();
    if !skill.paths.is_empty() {
        tags.push(format!("paths:{}", skill.paths.join(",")));
    }
    if skill.context == SkillContextMode::Fork {
        tags.push("context:fork".to_string());
    }
    if let Some(model) = &skill.model {
        tags.push(format!("model:{}", model));
    }
    if tags.is_empty() {
        String::new()
    } else {
        format!(" [{}]", tags.join(" | "))
    }
}

fn render_skill_metadata_block(skill: &SkillEntry) -> String {
    let mut lines = Vec::new();
    if !skill.allowed_tools.is_empty() {
        lines.push(format!(
            "- Allowed tools: {}",
            skill.allowed_tools.join(", ")
        ));
    }
    if !skill.paths.is_empty() {
        lines.push(format!("- Path activation: {}", skill.paths.join(", ")));
    }
    if skill.context == SkillContextMode::Fork {
        lines.push("- Context mode: fork".to_string());
    }
    if let Some(model) = &skill.model {
        lines.push(format!("- Preferred model: {}", model));
    }
    if let Some(effort) = &skill.effort {
        lines.push(format!("- Preferred effort: {}", effort));
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("Skill metadata:\n{}", lines.join("\n"))
    }
}

fn skill_metadata_json(skill: &SkillEntry) -> serde_json::Value {
    json!({
        "name": skill.name,
        "description": skill.description,
        "allowed_tools": skill.allowed_tools,
        "paths": skill.paths,
        "context": skill.context.label(),
        "model": skill.model,
        "effort": skill.effort,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::Mutex;

    use crate::tool::{SubAgentOptions, SubAgentRunner, Tool, ToolContext};

    use super::{SkillStore, SkillTool};
    use std::pin::Pin;
    use std::sync::Mutex as StdMutex;

    struct MockRunner {
        seen: Arc<StdMutex<Vec<(String, SubAgentOptions)>>>,
    }

    impl SubAgentRunner for MockRunner {
        fn run_sub_agent(
            &self,
            prompt: String,
            options: SubAgentOptions,
        ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>>
        {
            self.seen.lock().unwrap().push((prompt, options));
            Box::pin(async { Ok("skill run ok".to_string()) })
        }
    }

    #[tokio::test]
    async fn skill_get_returns_skill_content() {
        let store = Arc::new(Mutex::new(SkillStore::new()));
        {
            let mut guard = store.lock().await;
            guard.add(
                "rust".to_string(),
                "Rust guidance".to_string(),
                "Prefer cargo test.".to_string(),
            );
        }

        let result = SkillTool { store }
            .execute(
                json!({
                    "name": "rust",
                    "action": "get"
                }),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert_eq!(result.content, "Prefer cargo test.");
        assert_eq!(result.metadata.as_ref().unwrap()["name"], json!("rust"));
    }

    #[tokio::test]
    async fn skill_list_reports_available_skills() {
        let store = Arc::new(Mutex::new(SkillStore::new()));
        {
            let mut guard = store.lock().await;
            guard.add(
                "rust".to_string(),
                "Rust guidance".to_string(),
                "Prefer cargo test.".to_string(),
            );
            guard.add(
                "python".to_string(),
                "Python guidance".to_string(),
                "Prefer pytest.".to_string(),
            );
        }

        let result = SkillTool { store }
            .execute(
                json!({"action":"list","name":"ignored"}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("/rust"));
        assert!(result.content.contains("/python"));
        assert_eq!(result.metadata.as_ref().unwrap()["count"], json!(2));
    }

    #[tokio::test]
    async fn skill_get_includes_metadata_guidance() {
        let store = Arc::new(Mutex::new(SkillStore::new()));
        {
            let mut guard = store.lock().await;
            guard.add_entry(super::SkillEntry {
                name: "review".to_string(),
                description: "Review guidance".to_string(),
                content: "Inspect the diff.".to_string(),
                allowed_tools: vec!["git_diff".to_string()],
                paths: vec!["crates/**".to_string()],
                context: super::SkillContextMode::Fork,
                model: Some("claude-sonnet".to_string()),
                effort: Some("high".to_string()),
            });
        }

        let result = SkillTool { store }
            .execute(
                json!({
                    "name": "review",
                    "action": "get"
                }),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("Allowed tools: git_diff"));
        assert!(result.content.contains("Context mode: fork"));
        assert!(result.content.contains("Inspect the diff."));
    }

    #[tokio::test]
    async fn skill_run_uses_sub_agent_for_fork_context() {
        let store = Arc::new(Mutex::new(SkillStore::new()));
        {
            let mut guard = store.lock().await;
            guard.add_entry(super::SkillEntry {
                name: "review".to_string(),
                description: "Review guidance".to_string(),
                content: "Inspect the diff.".to_string(),
                allowed_tools: vec!["git_diff".to_string()],
                paths: vec!["crates/**".to_string()],
                context: super::SkillContextMode::Fork,
                model: Some("claude-sonnet".to_string()),
                effort: Some("high".to_string()),
            });
        }
        let seen = Arc::new(StdMutex::new(Vec::new()));
        let mut ctx = ToolContext::empty();
        ctx.sub_agent_runner = Some(Arc::new(MockRunner {
            seen: Arc::clone(&seen),
        }));

        let result = SkillTool { store }
            .execute(
                json!({
                    "name": "review",
                    "action": "run",
                    "prompt": "check regressions"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert_eq!(result.content, "skill run ok");
        assert_eq!(result.metadata.as_ref().unwrap()["mode"], json!("fork"));
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].0.contains("Inspect the diff."));
        assert!(seen[0].0.contains("check regressions"));
        assert_eq!(seen[0].1.allowed_tools, vec!["git_diff".to_string()]);
        assert_eq!(seen[0].1.model.as_deref(), Some("claude-sonnet"));
    }

    #[tokio::test]
    async fn skill_run_reports_missing_runner_for_fork_context() {
        let store = Arc::new(Mutex::new(SkillStore::new()));
        {
            let mut guard = store.lock().await;
            guard.add_entry(super::SkillEntry {
                name: "review".to_string(),
                description: "Review guidance".to_string(),
                content: "Inspect the diff.".to_string(),
                allowed_tools: vec![],
                paths: vec![],
                context: super::SkillContextMode::Fork,
                model: None,
                effort: None,
            });
        }

        let result = SkillTool { store }
            .execute(
                json!({
                    "name": "review",
                    "action": "run"
                }),
                &ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("no sub-agent runner"));
    }
}
