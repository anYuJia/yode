use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

/// Global hypothesis store (session-scoped via lazy_static-like pattern).
/// We use a module-level Mutex since ToolContext doesn't have a dedicated field for this.
static HYPOTHESIS_STORE: std::sync::LazyLock<Mutex<HypothesisStore>> =
    std::sync::LazyLock::new(|| Mutex::new(HypothesisStore::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HypothesisStatus {
    Pending,
    Verified,
    Refuted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FindingType {
    Bug,
    Risk,
    Optimization,
    DesignChoice,
}

impl FindingType {
    fn label(&self) -> &str {
        match self {
            FindingType::Bug => "BUG",
            FindingType::Risk => "RISK",
            FindingType::Optimization => "OPTIMIZATION",
            FindingType::DesignChoice => "DESIGN_CHOICE",
        }
    }

    fn icon(&self) -> &str {
        match self {
            FindingType::Bug => "🔴",
            FindingType::Risk => "⚠️",
            FindingType::Optimization => "💡",
            FindingType::DesignChoice => "📐",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    fn label(&self) -> &str {
        match self {
            Confidence::High => "HIGH",
            Confidence::Medium => "MEDIUM",
            Confidence::Low => "LOW",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: String,
    pub hypothesis: String,
    pub evidence_needed: String,
    pub finding_type: FindingType,
    pub status: HypothesisStatus,
    pub evidence: Option<String>,
    pub confidence: Option<Confidence>,
}

#[derive(Debug, Default)]
pub struct HypothesisStore {
    hypotheses: HashMap<String, Hypothesis>,
    next_id: u64,
}

impl HypothesisStore {
    fn new() -> Self {
        Self {
            hypotheses: HashMap::new(),
            next_id: 1,
        }
    }

    fn create(
        &mut self,
        hypothesis: String,
        evidence_needed: String,
        finding_type: FindingType,
    ) -> Hypothesis {
        let id = format!("h{}", self.next_id);
        self.next_id += 1;
        let h = Hypothesis {
            id: id.clone(),
            hypothesis,
            evidence_needed,
            finding_type,
            status: HypothesisStatus::Pending,
            evidence: None,
            confidence: None,
        };
        self.hypotheses.insert(id, h.clone());
        h
    }

    fn verify(
        &mut self,
        id: &str,
        evidence: String,
        confidence: Confidence,
    ) -> Option<&Hypothesis> {
        if let Some(h) = self.hypotheses.get_mut(id) {
            h.status = HypothesisStatus::Verified;
            h.evidence = Some(evidence);
            h.confidence = Some(confidence);
            Some(h)
        } else {
            None
        }
    }

    fn refute(&mut self, id: &str, evidence: String) -> Option<&Hypothesis> {
        if let Some(h) = self.hypotheses.get_mut(id) {
            h.status = HypothesisStatus::Refuted;
            h.evidence = Some(evidence);
            Some(h)
        } else {
            None
        }
    }

    fn list(&self) -> Vec<&Hypothesis> {
        let mut result: Vec<&Hypothesis> = self.hypotheses.values().collect();
        result.sort_by(|a, b| a.id.cmp(&b.id));
        result
    }

    fn generate_report(&self) -> String {
        let mut output = String::new();
        output.push_str("# Analysis Report\n\n");

        // Group verified findings by type
        let verified: Vec<&Hypothesis> = self
            .hypotheses
            .values()
            .filter(|h| matches!(h.status, HypothesisStatus::Verified))
            .collect();

        let refuted: Vec<&Hypothesis> = self
            .hypotheses
            .values()
            .filter(|h| matches!(h.status, HypothesisStatus::Refuted))
            .collect();

        let pending: Vec<&Hypothesis> = self
            .hypotheses
            .values()
            .filter(|h| matches!(h.status, HypothesisStatus::Pending))
            .collect();

        if verified.is_empty() && refuted.is_empty() && pending.is_empty() {
            output.push_str("No hypotheses recorded.\n");
            return output;
        }

        // Verified findings grouped by type
        output.push_str("## Verified Findings\n\n");
        if verified.is_empty() {
            output.push_str("None.\n\n");
        } else {
            // Sort by type, then by confidence (HIGH first)
            let mut sorted = verified.clone();
            sorted.sort_by(|a, b| {
                let type_order = |t: &FindingType| match t {
                    FindingType::Bug => 0,
                    FindingType::Risk => 1,
                    FindingType::Optimization => 2,
                    FindingType::DesignChoice => 3,
                };
                type_order(&a.finding_type).cmp(&type_order(&b.finding_type))
            });

            for h in &sorted {
                output.push_str(&format!(
                    "### {} {} [{}] {}\n",
                    h.finding_type.icon(),
                    h.finding_type.label(),
                    h.id,
                    h.hypothesis
                ));
                if let Some(ref conf) = h.confidence {
                    output.push_str(&format!("- **Confidence**: {}\n", conf.label()));
                }
                if let Some(ref ev) = h.evidence {
                    output.push_str(&format!("- **Evidence**: {}\n", ev));
                }
                output.push('\n');
            }
        }

        // Refuted hypotheses
        if !refuted.is_empty() {
            output.push_str("## Excluded (Refuted)\n\n");
            for h in &refuted {
                output.push_str(&format!(
                    "- [{}] \"{}\" — REFUTED: {}\n",
                    h.id,
                    h.hypothesis,
                    h.evidence.as_deref().unwrap_or("no evidence recorded")
                ));
            }
            output.push('\n');
        }

        // Pending hypotheses
        if !pending.is_empty() {
            output.push_str("## Pending (Not Yet Verified)\n\n");
            for h in &pending {
                output.push_str(&format!(
                    "- [{}] {} (needs: {})\n",
                    h.id, h.hypothesis, h.evidence_needed
                ));
            }
            output.push('\n');
        }

        output
    }

    fn clear(&mut self) {
        self.hypotheses.clear();
        self.next_id = 1;
    }
}

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
            .and_then(|v| v.as_str())
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

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: action"))?;

        match action {
            "create" => {
                let hypothesis = params
                    .get("hypothesis")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing: hypothesis"))?;
                let evidence_needed = params
                    .get("evidence_needed")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing: evidence_needed"))?;
                let finding_type = parse_finding_type(
                    params
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("RISK"),
                );

                let mut store = HYPOTHESIS_STORE.lock().unwrap();
                let h = store.create(
                    hypothesis.to_string(),
                    evidence_needed.to_string(),
                    finding_type,
                );

                Ok(ToolResult::success(format!(
                    "Created hypothesis [{}]: {}\nNeeds: {}",
                    h.id, h.hypothesis, h.evidence_needed
                )))
            }
            "verify" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing: id"))?;
                let evidence = params
                    .get("evidence")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing: evidence"))?;
                let confidence = parse_confidence(
                    params
                        .get("confidence")
                        .and_then(|v| v.as_str())
                        .unwrap_or("MEDIUM"),
                );

                let mut store = HYPOTHESIS_STORE.lock().unwrap();
                match store.verify(id, evidence.to_string(), confidence) {
                    Some(h) => Ok(ToolResult::success(format!(
                        "Verified [{}]: {}\nEvidence: {}",
                        h.id,
                        h.hypothesis,
                        h.evidence.as_deref().unwrap_or("")
                    ))),
                    None => Ok(ToolResult::error(format!("Hypothesis '{}' not found.", id))),
                }
            }
            "refute" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing: id"))?;
                let evidence = params
                    .get("evidence")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing: evidence"))?;

                let mut store = HYPOTHESIS_STORE.lock().unwrap();
                match store.refute(id, evidence.to_string()) {
                    Some(h) => Ok(ToolResult::success(format!(
                        "Refuted [{}]: {}\nReason: {}",
                        h.id,
                        h.hypothesis,
                        h.evidence.as_deref().unwrap_or("")
                    ))),
                    None => Ok(ToolResult::error(format!("Hypothesis '{}' not found.", id))),
                }
            }
            "list" => {
                let store = HYPOTHESIS_STORE.lock().unwrap();
                let hypotheses = store.list();
                if hypotheses.is_empty() {
                    return Ok(ToolResult::success("No hypotheses recorded.".to_string()));
                }
                let mut output = String::new();
                for h in hypotheses {
                    let status = match h.status {
                        HypothesisStatus::Pending => "PENDING",
                        HypothesisStatus::Verified => "VERIFIED",
                        HypothesisStatus::Refuted => "REFUTED",
                    };
                    output.push_str(&format!(
                        "[{}] {} ({}) — {}\n",
                        h.id,
                        h.hypothesis,
                        h.finding_type.label(),
                        status
                    ));
                }
                Ok(ToolResult::success(output))
            }
            "report" => {
                let store = HYPOTHESIS_STORE.lock().unwrap();
                Ok(ToolResult::success(store.generate_report()))
            }
            "clear" => {
                let mut store = HYPOTHESIS_STORE.lock().unwrap();
                store.clear();
                Ok(ToolResult::success("All hypotheses cleared.".to_string()))
            }
            _ => Ok(ToolResult::error(format!(
                "Unknown action: '{}'. Use create, verify, refute, list, report, or clear.",
                action
            ))),
        }
    }
}

fn parse_finding_type(s: &str) -> FindingType {
    match s.to_uppercase().as_str() {
        "BUG" => FindingType::Bug,
        "RISK" => FindingType::Risk,
        "OPTIMIZATION" => FindingType::Optimization,
        "DESIGN_CHOICE" => FindingType::DesignChoice,
        _ => FindingType::Risk,
    }
}

fn parse_confidence(s: &str) -> Confidence {
    match s.to_uppercase().as_str() {
        "HIGH" => Confidence::High,
        "LOW" => Confidence::Low,
        _ => Confidence::Medium,
    }
}
