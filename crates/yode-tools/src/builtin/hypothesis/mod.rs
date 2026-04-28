mod store;
mod types;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

use self::store::{hypothesis_session_key, with_hypothesis_store};
use self::types::{parse_confidence, parse_finding_type, HypothesisStatus};

pub struct HypothesisTool;

#[async_trait]
impl Tool for HypothesisTool {
    fn name(&self) -> &str {
        "hypothesis"
    }

    fn user_facing_name(&self) -> &str {
        "Hypothesis"
    }

    fn activity_description(&self, params: &Value) -> String {
        let action = params
            .get("action")
            .and_then(|value| value.as_str())
            .unwrap_or("manage");
        format!("Hypothesis: {} action", action)
    }

    fn description(&self) -> &str {
        "Track and verify hypotheses during code analysis. Actions: \
         create (form a hypothesis), verify (confirm with evidence), \
         refute (disprove with evidence), list (show all), \
         report (generate structured analysis report), clear (reset all). \
         Use this to enforce evidence-based reasoning during analysis."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "verify", "refute", "list", "report", "clear"],
                    "description": "The action to perform"
                },
                "id": {
                    "type": "string",
                    "description": "Hypothesis ID (required for verify/refute)"
                },
                "hypothesis": {
                    "type": "string",
                    "description": "The hypothesis statement (required for create)"
                },
                "evidence_needed": {
                    "type": "string",
                    "description": "What evidence would confirm/refute this (required for create)"
                },
                "evidence": {
                    "type": "string",
                    "description": "The evidence found (required for verify/refute)"
                },
                "type": {
                    "type": "string",
                    "enum": ["BUG", "RISK", "OPTIMIZATION", "DESIGN_CHOICE"],
                    "description": "Finding type (required for create)"
                },
                "confidence": {
                    "type": "string",
                    "enum": ["HIGH", "MEDIUM", "LOW"],
                    "description": "Confidence level (required for verify)"
                }
            },
            "required": ["action"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            read_only: true,
            requires_confirmation: false,
            supports_auto_execution: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let action = params
            .get("action")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: action"))?;
        let session_key = hypothesis_session_key(ctx.session_id.as_deref());

        match action {
            "create" => self.create_hypothesis(&session_key, &params),
            "verify" => self.verify_hypothesis(&session_key, &params),
            "refute" => self.refute_hypothesis(&session_key, &params),
            "list" => self.list_hypotheses(&session_key),
            "report" => self.generate_report(&session_key),
            "clear" => self.clear_hypotheses(&session_key),
            _ => Ok(ToolResult::error(format!(
                "Unknown action: '{}'. Use create, verify, refute, list, report, or clear.",
                action
            ))),
        }
    }
}

impl HypothesisTool {
    fn create_hypothesis(&self, session_key: &str, params: &Value) -> Result<ToolResult> {
        let hypothesis = params
            .get("hypothesis")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing: hypothesis"))?;
        let evidence_needed = params
            .get("evidence_needed")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing: evidence_needed"))?;
        let finding_type = parse_finding_type(
            params
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("RISK"),
        );

        let hypothesis = with_hypothesis_store(session_key, |store| {
            store.create(
                hypothesis.to_string(),
                evidence_needed.to_string(),
                finding_type,
            )
        });

        Ok(ToolResult::success(format!(
            "Created hypothesis [{}]: {}\nNeeds: {}",
            hypothesis.id, hypothesis.hypothesis, hypothesis.evidence_needed
        )))
    }

    fn verify_hypothesis(&self, session_key: &str, params: &Value) -> Result<ToolResult> {
        let id = params
            .get("id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing: id"))?;
        let evidence = params
            .get("evidence")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing: evidence"))?;
        let confidence = parse_confidence(
            params
                .get("confidence")
                .and_then(|value| value.as_str())
                .unwrap_or("MEDIUM"),
        );

        with_hypothesis_store(session_key, |store| {
            match store.verify(id, evidence.to_string(), confidence) {
                Some(hypothesis) => Ok(ToolResult::success(format!(
                    "Verified [{}]: {}\nEvidence: {}",
                    hypothesis.id,
                    hypothesis.hypothesis,
                    hypothesis.evidence.as_deref().unwrap_or("")
                ))),
                None => Ok(ToolResult::error(format!("Hypothesis '{}' not found.", id))),
            }
        })
    }

    fn refute_hypothesis(&self, session_key: &str, params: &Value) -> Result<ToolResult> {
        let id = params
            .get("id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing: id"))?;
        let evidence = params
            .get("evidence")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing: evidence"))?;

        with_hypothesis_store(session_key, |store| {
            match store.refute(id, evidence.to_string()) {
                Some(hypothesis) => Ok(ToolResult::success(format!(
                    "Refuted [{}]: {}\nReason: {}",
                    hypothesis.id,
                    hypothesis.hypothesis,
                    hypothesis.evidence.as_deref().unwrap_or("")
                ))),
                None => Ok(ToolResult::error(format!("Hypothesis '{}' not found.", id))),
            }
        })
    }

    fn list_hypotheses(&self, session_key: &str) -> Result<ToolResult> {
        with_hypothesis_store(session_key, |store| {
            let hypotheses = store.list();
            if hypotheses.is_empty() {
                return Ok(ToolResult::success("No hypotheses recorded.".to_string()));
            }

            let mut output = String::new();
            for hypothesis in hypotheses {
                let status = match hypothesis.status {
                    HypothesisStatus::Pending => "PENDING",
                    HypothesisStatus::Verified => "VERIFIED",
                    HypothesisStatus::Refuted => "REFUTED",
                };
                output.push_str(&format!(
                    "[{}] {} ({}) — {}\n",
                    hypothesis.id,
                    hypothesis.hypothesis,
                    hypothesis.finding_type.label(),
                    status
                ));
            }
            Ok(ToolResult::success(output))
        })
    }

    fn generate_report(&self, session_key: &str) -> Result<ToolResult> {
        Ok(ToolResult::success(with_hypothesis_store(
            session_key,
            |store| store.generate_report(),
        )))
    }

    fn clear_hypotheses(&self, session_key: &str) -> Result<ToolResult> {
        with_hypothesis_store(session_key, |store| store.clear());
        Ok(ToolResult::success("All hypotheses cleared.".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::HypothesisTool;
    use crate::tool::{Tool, ToolContext};

    fn ctx(session_id: &str) -> ToolContext {
        let mut ctx = ToolContext::empty();
        ctx.session_id = Some(session_id.to_string());
        ctx
    }

    #[tokio::test]
    async fn hypotheses_are_isolated_by_session_id() {
        let tool = HypothesisTool;
        let session_a = ctx("session-a");
        let session_b = ctx("session-b");

        tool.execute(
            json!({
                "action": "clear"
            }),
            &session_a,
        )
        .await
        .unwrap();
        tool.execute(
            json!({
                "action": "clear"
            }),
            &session_b,
        )
        .await
        .unwrap();
        tool.execute(
            json!({
                "action": "create",
                "hypothesis": "session A only",
                "evidence_needed": "proof",
                "type": "RISK"
            }),
            &session_a,
        )
        .await
        .unwrap();

        let list_a = tool
            .execute(json!({"action": "list"}), &session_a)
            .await
            .unwrap();
        let list_b = tool
            .execute(json!({"action": "list"}), &session_b)
            .await
            .unwrap();

        assert!(list_a.content.contains("session A only"));
        assert_eq!(list_b.content, "No hypotheses recorded.");
    }
}
