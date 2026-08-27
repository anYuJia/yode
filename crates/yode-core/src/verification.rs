use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Test,
    StaticAnalysis,
    Runtime,
    DiffReview,
    Browser,
    Security,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptanceCriterion {
    pub id: String,
    pub description: String,
    pub required: bool,
    #[serde(default)]
    pub evidence_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationEvidence {
    pub criterion_id: String,
    pub kind: EvidenceKind,
    pub source: String,
    pub success: bool,
    pub detail: String,
    #[serde(default)]
    pub artifact_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationSummary {
    pub criteria_total: u32,
    pub required_total: u32,
    pub required_passed: u32,
    pub required_failed: u32,
    pub required_unverified: u32,
    pub evidence_count: u32,
    pub can_deliver: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationGate {
    criteria: BTreeMap<String, AcceptanceCriterion>,
    evidence: Vec<VerificationEvidence>,
}

impl VerificationGate {
    pub fn new(criteria: Vec<AcceptanceCriterion>) -> Result<Self> {
        let mut gate = Self::default();
        for criterion in criteria {
            gate.add_criterion(criterion)?;
        }
        Ok(gate)
    }

    pub fn add_criterion(&mut self, criterion: AcceptanceCriterion) -> Result<()> {
        let id = criterion.id.trim();
        if id.is_empty() {
            return Err(anyhow!("acceptance criterion id cannot be empty"));
        }
        if criterion.description.trim().is_empty() {
            return Err(anyhow!("acceptance criterion description cannot be empty"));
        }
        if self.criteria.contains_key(id) {
            return Err(anyhow!("duplicate acceptance criterion id '{}'", id));
        }
        self.criteria.insert(id.to_string(), criterion);
        Ok(())
    }

    pub fn record_evidence(&mut self, evidence: VerificationEvidence) -> Result<()> {
        if !self.criteria.contains_key(evidence.criterion_id.trim()) {
            return Err(anyhow!(
                "evidence references unknown acceptance criterion '{}'",
                evidence.criterion_id
            ));
        }
        if evidence.source.trim().is_empty() {
            return Err(anyhow!("verification evidence source cannot be empty"));
        }
        self.evidence.push(evidence);
        Ok(())
    }

    pub fn criteria(&self) -> impl Iterator<Item = &AcceptanceCriterion> {
        self.criteria.values()
    }

    pub fn evidence(&self) -> &[VerificationEvidence] {
        &self.evidence
    }

    pub fn summary(&self) -> VerificationSummary {
        let mut summary = VerificationSummary {
            criteria_total: self.criteria.len() as u32,
            required_total: self
                .criteria
                .values()
                .filter(|criterion| criterion.required)
                .count() as u32,
            evidence_count: self.evidence.len() as u32,
            ..VerificationSummary::default()
        };

        let mut seen_required = BTreeSet::new();
        for criterion in self
            .criteria
            .values()
            .filter(|criterion| criterion.required)
        {
            let criterion_evidence = self
                .evidence
                .iter()
                .filter(|evidence| evidence.criterion_id == criterion.id)
                .collect::<Vec<_>>();
            if criterion_evidence.is_empty() {
                summary.required_unverified += 1;
                continue;
            }
            seen_required.insert(criterion.id.as_str());
            if criterion_evidence.iter().any(|evidence| !evidence.success) {
                summary.required_failed += 1;
            } else if criterion_evidence.iter().any(|evidence| evidence.success) {
                summary.required_passed += 1;
            } else {
                summary.required_unverified += 1;
            }
        }

        debug_assert_eq!(
            seen_required.len() as u32,
            summary.required_passed + summary.required_failed
        );
        summary.can_deliver = summary.required_total > 0
            && summary.required_failed == 0
            && summary.required_unverified == 0
            && summary.required_passed == summary.required_total;
        summary
    }

    pub fn can_deliver(&self) -> bool {
        self.summary().can_deliver
    }

    pub fn missing_required_ids(&self) -> Vec<String> {
        self.criteria
            .values()
            .filter(|criterion| criterion.required)
            .filter(|criterion| {
                !self
                    .evidence
                    .iter()
                    .any(|evidence| evidence.criterion_id == criterion.id && evidence.success)
            })
            .map(|criterion| criterion.id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn criterion(id: &str) -> AcceptanceCriterion {
        AcceptanceCriterion {
            id: id.to_string(),
            description: format!("verify {id}"),
            required: true,
            evidence_hint: None,
        }
    }

    #[test]
    fn delivery_requires_all_required_criteria() {
        let mut gate = VerificationGate::new(vec![criterion("build"), criterion("tests")]).unwrap();
        gate.record_evidence(VerificationEvidence {
            criterion_id: "build".to_string(),
            kind: EvidenceKind::Test,
            source: "cargo check".to_string(),
            success: true,
            detail: "ok".to_string(),
            artifact_path: None,
        })
        .unwrap();
        assert!(!gate.can_deliver());
        assert_eq!(gate.missing_required_ids(), vec!["tests"]);

        gate.record_evidence(VerificationEvidence {
            criterion_id: "tests".to_string(),
            kind: EvidenceKind::Test,
            source: "cargo test".to_string(),
            success: true,
            detail: "all passed".to_string(),
            artifact_path: None,
        })
        .unwrap();
        assert!(gate.can_deliver());
    }

    #[test]
    fn failed_evidence_blocks_delivery() {
        let mut gate = VerificationGate::new(vec![criterion("tests")]).unwrap();
        gate.record_evidence(VerificationEvidence {
            criterion_id: "tests".to_string(),
            kind: EvidenceKind::Test,
            source: "cargo test".to_string(),
            success: false,
            detail: "one failed".to_string(),
            artifact_path: None,
        })
        .unwrap();
        let summary = gate.summary();
        assert_eq!(summary.required_failed, 1);
        assert!(!summary.can_deliver);
    }

    #[test]
    fn evidence_cannot_reference_unknown_criterion() {
        let mut gate = VerificationGate::new(vec![criterion("tests")]).unwrap();
        assert!(gate
            .record_evidence(VerificationEvidence {
                criterion_id: "unknown".to_string(),
                kind: EvidenceKind::Manual,
                source: "manual".to_string(),
                success: true,
                detail: "ok".to_string(),
                artifact_path: None,
            })
            .is_err());
    }
}
