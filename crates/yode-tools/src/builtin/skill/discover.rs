use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::builtin::skill::SkillStore;
use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

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

    fn activity_description(&self, params: &Value) -> String {
        if let Some(query) = params.get("query").and_then(|v| v.as_str()) {
            format!("Searching skills for {}", query)
        } else {
            "Discovering available skills".to_string()
        }
    }

    fn description(&self) -> &str {
        "Discover available skills and their capabilities. Skills provide pre-defined workflows or domain-specific knowledge."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Optional search query. Matches skill names, descriptions, path patterns, and trigger examples."
                }
            },
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

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let store = self.store.lock().await;
        let total_skills = store.list().len();
        let results = store.search(query);

        if total_skills == 0 {
            return Ok(ToolResult::success_with_metadata(
                "No skills discovered in the current workspace.".to_string(),
                json!({
                    "query": query,
                    "count": 0,
                    "total": 0,
                    "skills": []
                }),
            ));
        }

        if !query.is_empty() && results.is_empty() {
            return Ok(ToolResult::success_with_metadata(
                format!("No skills match query '{}'.", query),
                json!({
                    "query": query,
                    "count": 0,
                    "total": total_skills,
                    "skills": []
                }),
            ));
        }

        let mut output = if query.is_empty() {
            String::from("Available skills:\n\n")
        } else {
            format!("Matching skills for '{}':\n\n", query)
        };
        for result in &results {
            let skill = &result.skill;
            output.push_str(&format!("- **{}**: {}\n", skill.name, skill.description));
            let mut details = Vec::new();
            if !query.is_empty() {
                details.push(format!("score={}", result.score));
                if !result.reasons.is_empty() {
                    details.push(format!("reasons={}", result.reasons.join(",")));
                }
            }
            if !skill.paths.is_empty() {
                details.push(format!("paths={}", skill.paths.join(",")));
            }
            if !skill.trigger_examples.is_empty() {
                details.push(format!(
                    "trigger-examples={}",
                    skill.trigger_examples.join(" | ")
                ));
            }
            if !skill.allowed_tools.is_empty() {
                details.push(format!("allowed-tools={}", skill.allowed_tools.join(",")));
            }
            if skill.context != crate::builtin::skill::SkillContextMode::Inline {
                details.push(format!("context={}", skill.context.label()));
            }
            if let Some(model) = &skill.model {
                details.push(format!("model={}", model));
            }
            if let Some(effort) = &skill.effort {
                details.push(format!("effort={}", effort));
            }
            if !details.is_empty() {
                output.push_str(&format!("  - {}\n", details.join(" | ")));
            }
        }

        Ok(ToolResult::success_with_metadata(
            output,
            json!({
                "query": query,
                "count": results.len(),
                "total": total_skills,
                "skills": results.iter().map(|result| {
                    json!({
                        "name": result.skill.name,
                        "description": result.skill.description,
                        "score": result.score,
                        "reasons": result.reasons,
                        "paths": result.skill.paths,
                        "trigger_examples": result.skill.trigger_examples,
                    })
                }).collect::<Vec<_>>()
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Mutex;

    use crate::tool::Tool;

    use super::DiscoverSkillsTool;
    use crate::builtin::skill::{SkillContextMode, SkillEntry, SkillStore};

    #[tokio::test]
    async fn discover_skills_lists_known_skills_deterministically() {
        let store = Arc::new(Mutex::new(SkillStore::new()));
        {
            let mut guard = store.lock().await;
            guard.add(
                "rust".to_string(),
                "Rust guidance".to_string(),
                "Prefer cargo test.".to_string(),
            );
            guard.add(
                "analysis".to_string(),
                "Analysis guidance".to_string(),
                "Prefer focused reads.".to_string(),
            );
        }

        let result = DiscoverSkillsTool { store }
            .execute(serde_json::json!({}), &crate::tool::ToolContext::empty())
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("rust"));
        assert!(result.content.contains("Rust guidance"));
        assert!(result.content.find("analysis").unwrap() < result.content.find("rust").unwrap());
    }

    #[tokio::test]
    async fn discover_skills_reports_empty_workspace() {
        let store = Arc::new(Mutex::new(SkillStore::new()));
        let result = DiscoverSkillsTool { store }
            .execute(serde_json::json!({}), &crate::tool::ToolContext::empty())
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("No skills discovered"));
    }

    #[tokio::test]
    async fn discover_skills_ranks_query_matches_with_reasons() {
        let store = Arc::new(Mutex::new(SkillStore::new()));
        {
            let mut guard = store.lock().await;
            guard.add_entry(SkillEntry {
                name: "rust-review".to_string(),
                description: "Review Rust changes".to_string(),
                content: "Review Rust code.".to_string(),
                allowed_tools: vec![],
                paths: vec!["crates/**/*.rs".to_string()],
                trigger_examples: vec!["when asked to audit rust changes".to_string()],
                context: SkillContextMode::Inline,
                model: None,
                effort: None,
            });
            guard.add_entry(SkillEntry {
                name: "docs".to_string(),
                description: "Documentation updates".to_string(),
                content: "Update docs.".to_string(),
                allowed_tools: vec![],
                paths: vec!["docs/**/*.md".to_string()],
                trigger_examples: vec!["when asked to explain rust concepts".to_string()],
                context: SkillContextMode::Inline,
                model: None,
                effort: None,
            });
            guard.add_entry(SkillEntry {
                name: "general".to_string(),
                description: "General guidance".to_string(),
                content: "Think carefully.".to_string(),
                allowed_tools: vec![],
                paths: vec![],
                trigger_examples: vec![],
                context: SkillContextMode::Inline,
                model: None,
                effort: None,
            });
        }

        let result = DiscoverSkillsTool { store }
            .execute(
                serde_json::json!({"query": "rust"}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("Matching skills for 'rust'"));
        assert!(result.content.contains("score="));
        assert!(result.content.contains("reasons="));
        assert!(result.content.contains("trigger-examples="));
        assert!(result.content.find("rust-review").unwrap() < result.content.find("docs").unwrap());
        assert!(!result.content.contains("general"));
        assert_eq!(
            result.metadata.as_ref().unwrap()["count"],
            serde_json::json!(2)
        );
        assert_eq!(
            result.metadata.as_ref().unwrap()["skills"][0]["name"],
            serde_json::json!("rust-review")
        );
    }

    #[tokio::test]
    async fn discover_skills_reports_no_query_matches() {
        let store = Arc::new(Mutex::new(SkillStore::new()));
        {
            let mut guard = store.lock().await;
            guard.add(
                "rust".to_string(),
                "Rust guidance".to_string(),
                "Prefer cargo test.".to_string(),
            );
        }

        let result = DiscoverSkillsTool { store }
            .execute(
                serde_json::json!({"query": "python"}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("No skills match query 'python'"));
        assert_eq!(
            result.metadata.as_ref().unwrap()["count"],
            serde_json::json!(0)
        );
        assert_eq!(
            result.metadata.as_ref().unwrap()["total"],
            serde_json::json!(1)
        );
    }
}
