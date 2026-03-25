use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::tool::{Tool, ToolContext, ToolResult};

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
                    return Ok(ToolResult::success(
                        "No skills found. Add .md files to .yode/skills/ or ~/.yode/skills/.".to_string(),
                    ));
                }
                let mut output = String::from("Available skills:\n");
                for skill in skills {
                    output.push_str(&format!("  /{} — {}\n", skill.name, skill.description));
                }
                Ok(ToolResult::success(output))
            }
            "get" | _ => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;

                match store.get(name) {
                    Some(skill) => Ok(ToolResult::success(skill.content.clone())),
                    None => {
                        let available: Vec<&str> = store.list().iter().map(|s| s.name.as_str()).collect();
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
