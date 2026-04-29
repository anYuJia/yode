use std::collections::HashMap;
use std::sync::Mutex;

use super::types::{Confidence, FindingType, Hypothesis, HypothesisStatus};

static HYPOTHESIS_STORES: std::sync::LazyLock<Mutex<HashMap<String, HypothesisStore>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) fn hypothesis_session_key(session_id: Option<&str>) -> String {
    session_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or("__default__")
        .to_string()
}

pub(super) fn with_hypothesis_store<R>(
    session_key: &str,
    action: impl FnOnce(&mut HypothesisStore) -> R,
) -> R {
    let mut stores = HYPOTHESIS_STORES.lock().unwrap();
    let store = stores.entry(session_key.to_string()).or_default();
    action(store)
}

#[derive(Debug, Default)]
pub(super) struct HypothesisStore {
    hypotheses: HashMap<String, Hypothesis>,
    next_id: u64,
}

impl HypothesisStore {
    pub(super) fn create(
        &mut self,
        hypothesis: String,
        evidence_needed: String,
        finding_type: FindingType,
    ) -> Hypothesis {
        let id = format!("h{}", self.next_id);
        self.next_id += 1;
        let hypothesis = Hypothesis {
            id: id.clone(),
            hypothesis,
            evidence_needed,
            finding_type,
            status: HypothesisStatus::Pending,
            evidence: None,
            confidence: None,
        };
        self.hypotheses.insert(id, hypothesis.clone());
        hypothesis
    }

    pub(super) fn verify(
        &mut self,
        id: &str,
        evidence: String,
        confidence: Confidence,
    ) -> Option<&Hypothesis> {
        if let Some(hypothesis) = self.hypotheses.get_mut(id) {
            hypothesis.status = HypothesisStatus::Verified;
            hypothesis.evidence = Some(evidence);
            hypothesis.confidence = Some(confidence);
            Some(hypothesis)
        } else {
            None
        }
    }

    pub(super) fn refute(&mut self, id: &str, evidence: String) -> Option<&Hypothesis> {
        if let Some(hypothesis) = self.hypotheses.get_mut(id) {
            hypothesis.status = HypothesisStatus::Refuted;
            hypothesis.evidence = Some(evidence);
            Some(hypothesis)
        } else {
            None
        }
    }

    pub(super) fn list(&self) -> Vec<&Hypothesis> {
        let mut result: Vec<&Hypothesis> = self.hypotheses.values().collect();
        result.sort_by(|a, b| a.id.cmp(&b.id));
        result
    }

    pub(super) fn generate_report(&self) -> String {
        let mut output = String::new();
        output.push_str("# Analysis Report\n\n");

        let verified: Vec<&Hypothesis> = self
            .hypotheses
            .values()
            .filter(|hypothesis| matches!(hypothesis.status, HypothesisStatus::Verified))
            .collect();
        let refuted: Vec<&Hypothesis> = self
            .hypotheses
            .values()
            .filter(|hypothesis| matches!(hypothesis.status, HypothesisStatus::Refuted))
            .collect();
        let pending: Vec<&Hypothesis> = self
            .hypotheses
            .values()
            .filter(|hypothesis| matches!(hypothesis.status, HypothesisStatus::Pending))
            .collect();

        if verified.is_empty() && refuted.is_empty() && pending.is_empty() {
            output.push_str("No hypotheses recorded.\n");
            return output;
        }

        output.push_str("## Verified Findings\n\n");
        if verified.is_empty() {
            output.push_str("None.\n\n");
        } else {
            let mut sorted = verified.clone();
            sorted.sort_by(|a, b| {
                let type_order = |finding_type: &FindingType| match finding_type {
                    FindingType::Bug => 0,
                    FindingType::Risk => 1,
                    FindingType::Optimization => 2,
                    FindingType::DesignChoice => 3,
                };
                type_order(&a.finding_type).cmp(&type_order(&b.finding_type))
            });

            for hypothesis in &sorted {
                output.push_str(&format!(
                    "### {} {} [{}] {}\n",
                    hypothesis.finding_type.icon(),
                    hypothesis.finding_type.label(),
                    hypothesis.id,
                    hypothesis.hypothesis
                ));
                if let Some(confidence) = &hypothesis.confidence {
                    output.push_str(&format!("- **Confidence**: {}\n", confidence.label()));
                }
                if let Some(evidence) = &hypothesis.evidence {
                    output.push_str(&format!("- **Evidence**: {}\n", evidence));
                }
                output.push('\n');
            }
        }

        if !refuted.is_empty() {
            output.push_str("## Excluded (Refuted)\n\n");
            for hypothesis in &refuted {
                output.push_str(&format!(
                    "- [{}] \"{}\" — REFUTED: {}\n",
                    hypothesis.id,
                    hypothesis.hypothesis,
                    hypothesis
                        .evidence
                        .as_deref()
                        .unwrap_or("no evidence recorded")
                ));
            }
            output.push('\n');
        }

        if !pending.is_empty() {
            output.push_str("## Pending (Not Yet Verified)\n\n");
            for hypothesis in &pending {
                output.push_str(&format!(
                    "- [{}] {} (needs: {})\n",
                    hypothesis.id, hypothesis.hypothesis, hypothesis.evidence_needed
                ));
            }
            output.push('\n');
        }

        output
    }

    pub(super) fn clear(&mut self) {
        self.hypotheses.clear();
        self.next_id = 1;
    }
}
