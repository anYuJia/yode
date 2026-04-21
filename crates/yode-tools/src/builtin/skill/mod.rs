use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::tool::{Tool, ToolContext, ToolResult};

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
}

impl SkillStore {
    pub fn new() -> Self {
        Self { skills: Vec::new() }
    }

    pub fn add(&mut self, name: String, description: String, content: String) {
        self.skills.push(SkillEntry {
            name,
            description,
            content,
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
                    "enum": ["get", "list"],
                    "description": "Action to perform. 'get' loads a skill, 'list' lists all available skills. Default: 'get'"
                }
            },
            "required": ["name"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("get");

        let store = self.store.lock().await;

        match action {
            "list" => {
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
                    output.push_str(&format!("  /{} — {}\n", skill.name, skill.description));
                }
                let metadata = json!({ "action": "list", "count": skills.len() });
                Ok(ToolResult::success_with_metadata(output, metadata))
            }
            _ => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;

                match store.get(name) {
                    Some(skill) => {
                        let metadata = json!({ "action": "get", "name": name });
                        Ok(ToolResult::success_with_metadata(
                            skill.content.clone(),
                            metadata,
                        ))
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::Mutex;

    use crate::tool::Tool;

    use super::{SkillStore, SkillTool};

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
            .execute(json!({"action":"list","name":"ignored"}), &crate::tool::ToolContext::empty())
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("/rust"));
        assert!(result.content.contains("/python"));
        assert_eq!(result.metadata.as_ref().unwrap()["count"], json!(2));
    }
}
