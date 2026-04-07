use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};
use crate::builtin::skill::SkillStore;

pub struct DiscoverSkillsTool {
    pub store: Arc<Mutex<SkillStore>>,
}

#[async_trait]
impl Tool for DiscoverSkillsTool {
    fn name(&self) -> &str {
        "discover_skills"
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, _params: &Value) -> String {
        "Discovering available skills".to_string()
    }

    fn description(&self) -> &str {
        "Discover available skills and their capabilities. Skills provide pre-defined workflows or domain-specific knowledge."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let store = self.store.lock().await;
        let skills = store.list();
        
        let mut output = String::from("Available skills:\n\n");
        for skill in skills {
            output.push_str(&format!("- **{}**: {}\n", skill.name, skill.description));
        }
        
        if skills.is_empty() {
            output = "No skills discovered in the current workspace.".to_string();
        }

        Ok(ToolResult::success(output))
    }
}
