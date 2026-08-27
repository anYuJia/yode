use serde::{Deserialize, Serialize};

use crate::tool::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticToolMatch {
    pub name: String,
    pub score: i32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticToolRouter;

impl SemanticToolRouter {
    pub fn score(&self, query: &str, tool: &dyn Tool) -> Option<SemanticToolMatch> {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return None;
        }
        let tokens = tokenize(&query);
        let name = tool.name().to_ascii_lowercase();
        let description = tool.description().to_ascii_lowercase();
        let aliases = tool
            .aliases()
            .into_iter()
            .map(|alias| alias.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let capabilities = tool.capabilities();

        let mut score = 0i32;
        let mut reasons = Vec::new();

        if name == query {
            score += 180;
            reasons.push("exact tool name".to_string());
        } else if name.contains(&query) {
            score += 100;
            reasons.push("tool name contains query".to_string());
        }
        if aliases.iter().any(|alias| alias == &query) {
            score += 140;
            reasons.push("exact alias".to_string());
        }

        for token in &tokens {
            if name.split(['_', '-']).any(|segment| segment == token) {
                score += 55;
            } else if name.contains(token) {
                score += 35;
            }
            if aliases.iter().any(|alias| alias.contains(token)) {
                score += 28;
            }
            if description.contains(token) {
                score += 12;
            }
        }

        for (intent, tool_terms) in intent_terms(&tokens) {
            if tool_terms.iter().any(|term| name.contains(term)) {
                score += 65;
                reasons.push(format!("matches {intent} intent"));
            }
        }

        let read_intent = tokens.iter().any(|token| {
            matches!(
                token.as_str(),
                "read" | "inspect" | "search" | "find" | "list" | "show" | "查看" | "搜索" | "读取"
            )
        });
        let write_intent = tokens.iter().any(|token| {
            matches!(
                token.as_str(),
                "write" | "edit" | "change" | "modify" | "create" | "fix" | "写" | "修改" | "修复"
            )
        });
        if read_intent && capabilities.read_only {
            score += 18;
            reasons.push("read-only capability fits request".to_string());
        }
        if write_intent && !capabilities.read_only {
            score += 18;
            reasons.push("mutating capability fits request".to_string());
        }

        (score > 0).then(|| SemanticToolMatch {
            name: tool.name().to_string(),
            score,
            reasons,
        })
    }
}

fn tokenize(value: &str) -> Vec<String> {
    value
        .split(|ch: char| {
            !(ch.is_ascii_alphanumeric() || ch == '_' || ('\u{4e00}'..='\u{9fff}').contains(&ch))
        })
        .flat_map(|part| part.split('_'))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn intent_terms(tokens: &[String]) -> Vec<(&'static str, &'static [&'static str])> {
    let has = |candidates: &[&str]| {
        tokens
            .iter()
            .any(|token| candidates.contains(&token.as_str()))
    };
    let mut intents = Vec::new();
    if has(&["file", "read", "cat", "读取", "文件"]) {
        intents.push((
            "file-read",
            &["read_file", "file", "ls"] as &'static [&'static str],
        ));
    }
    if has(&["edit", "write", "modify", "change", "修改", "编辑", "写入"]) {
        intents.push((
            "file-edit",
            &["edit", "write", "multi_edit"] as &'static [&'static str],
        ));
    }
    if has(&["search", "find", "grep", "glob", "搜索", "查找"]) {
        intents.push((
            "search",
            &["grep", "glob", "search", "project_map"] as &'static [&'static str],
        ));
    }
    if has(&["test", "verify", "verification", "check", "测试", "验证"]) {
        intents.push((
            "verification",
            &["test", "verify", "verification", "review"] as &'static [&'static str],
        ));
    }
    if has(&["browser", "web", "page", "ui", "浏览器", "网页", "界面"]) {
        intents.push((
            "browser",
            &["browser", "web_fetch"] as &'static [&'static str],
        ));
    }
    if has(&["git", "commit", "diff", "branch", "提交", "分支"]) {
        intents.push(("git", &["git_", "worktree"] as &'static [&'static str]));
    }
    if has(&["agent", "subagent", "parallel", "team", "代理", "并行"]) {
        intents.push((
            "agent",
            &["agent", "team", "coordinator"] as &'static [&'static str],
        ));
    }
    if has(&["github", "issue", "pr", "pull", "ci"]) {
        intents.push((
            "github",
            &["github", "issue", "pr", "git"] as &'static [&'static str],
        ));
    }
    intents
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::{json, Value};

    use super::*;
    use crate::tool::{ToolCapabilities, ToolContext, ToolResult};

    struct Dummy {
        name: &'static str,
        description: &'static str,
        read_only: bool,
    }

    #[async_trait]
    impl Tool for Dummy {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            self.description
        }
        fn parameters_schema(&self) -> Value {
            json!({"type":"object"})
        }
        fn capabilities(&self) -> ToolCapabilities {
            ToolCapabilities {
                requires_confirmation: !self.read_only,
                supports_auto_execution: self.read_only,
                read_only: self.read_only,
            }
        }
        async fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
            Ok(ToolResult::success("ok".into()))
        }
    }

    #[test]
    fn semantic_intent_beats_unrelated_description() {
        let router = SemanticToolRouter;
        let edit = Dummy {
            name: "edit_file",
            description: "change source code safely",
            read_only: false,
        };
        let web = Dummy {
            name: "web_fetch",
            description: "fetch content",
            read_only: true,
        };
        let edit_score = router.score("modify file", &edit).unwrap().score;
        let web_score = router
            .score("modify file", &web)
            .map(|m| m.score)
            .unwrap_or_default();
        assert!(edit_score > web_score);
    }

    #[test]
    fn verification_intent_routes_to_test_tools() {
        let router = SemanticToolRouter;
        let test = Dummy {
            name: "test_runner",
            description: "run repository tests",
            read_only: false,
        };
        let score = router.score("verify the fix with tests", &test).unwrap();
        assert!(score
            .reasons
            .iter()
            .any(|reason| reason.contains("verification")));
    }
}
