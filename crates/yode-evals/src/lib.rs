use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvalTaskCategory {
    BugFix,
    MultiFileBug,
    Feature,
    Refactor,
    TestRepair,
    BuildFailure,
    DependencyMigration,
    FrontendUi,
    Security,
    RepositoryResearch,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptanceCriterion {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub evidence_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvalTask {
    pub id: String,
    pub title: String,
    pub category: EvalTaskCategory,
    pub prompt: String,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub base_revision: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub acceptance: Vec<AcceptanceCriterion>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl EvalTask {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(!self.id.trim().is_empty(), "eval task id cannot be empty");
        anyhow::ensure!(!self.title.trim().is_empty(), "eval task title cannot be empty");
        anyhow::ensure!(!self.prompt.trim().is_empty(), "eval task prompt cannot be empty");

        let mut ids = std::collections::BTreeSet::new();
        for criterion in &self.acceptance {
            anyhow::ensure!(
                !criterion.id.trim().is_empty(),
                "acceptance criterion id cannot be empty"
            );
            anyhow::ensure!(
                ids.insert(criterion.id.as_str()),
                "duplicate acceptance criterion id '{}'",
                criterion.id
            );
        }
        Ok(())
    }

    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read eval task {}", path.display()))?;
        let task: Self = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse eval task {}", path.display()))?;
        task.validate()?;
        Ok(task)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvalMetrics {
    pub wall_time_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_calls: u32,
    pub failed_tool_calls: u32,
    pub replans: u32,
    pub compactions: u32,
    pub subagents_launched: u32,
    pub user_interventions: u32,
    pub rollback_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CriterionStatus {
    Passed,
    Failed,
    NotRun,
    Inconclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CriterionResult {
    pub criterion_id: String,
    pub status: CriterionStatus,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvalOutcome {
    pub task_id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub success: bool,
    pub tests_passed: bool,
    pub regression_detected: bool,
    pub metrics: EvalMetrics,
    #[serde(default)]
    pub criteria: Vec<CriterionResult>,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EvalAggregateReport {
    pub runs: usize,
    pub successes: usize,
    pub regressions: usize,
    pub success_rate: f64,
    pub regression_rate: f64,
    pub average_wall_time_ms: f64,
    pub average_tool_calls: f64,
    pub average_tokens: f64,
    pub average_replans: f64,
    pub average_user_interventions: f64,
}

pub fn aggregate_outcomes(outcomes: &[EvalOutcome]) -> EvalAggregateReport {
    if outcomes.is_empty() {
        return EvalAggregateReport::default();
    }

    let runs = outcomes.len();
    let successes = outcomes.iter().filter(|outcome| outcome.success).count();
    let regressions = outcomes
        .iter()
        .filter(|outcome| outcome.regression_detected)
        .count();
    let sum_wall_time = outcomes
        .iter()
        .map(|outcome| outcome.metrics.wall_time_ms as f64)
        .sum::<f64>();
    let sum_tool_calls = outcomes
        .iter()
        .map(|outcome| outcome.metrics.tool_calls as f64)
        .sum::<f64>();
    let sum_tokens = outcomes
        .iter()
        .map(|outcome| (outcome.metrics.input_tokens + outcome.metrics.output_tokens) as f64)
        .sum::<f64>();
    let sum_replans = outcomes
        .iter()
        .map(|outcome| outcome.metrics.replans as f64)
        .sum::<f64>();
    let sum_interventions = outcomes
        .iter()
        .map(|outcome| outcome.metrics.user_interventions as f64)
        .sum::<f64>();
    let divisor = runs as f64;

    EvalAggregateReport {
        runs,
        successes,
        regressions,
        success_rate: successes as f64 / divisor,
        regression_rate: regressions as f64 / divisor,
        average_wall_time_ms: sum_wall_time / divisor,
        average_tool_calls: sum_tool_calls / divisor,
        average_tokens: sum_tokens / divisor,
        average_replans: sum_replans / divisor,
        average_user_interventions: sum_interventions / divisor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(success: bool, regression: bool, tool_calls: u32, tokens: u64) -> EvalOutcome {
        let now = Utc::now();
        EvalOutcome {
            task_id: "task".to_string(),
            started_at: now,
            finished_at: now,
            success,
            tests_passed: success,
            regression_detected: regression,
            metrics: EvalMetrics {
                wall_time_ms: 1_000,
                input_tokens: tokens,
                output_tokens: 0,
                tool_calls,
                ..EvalMetrics::default()
            },
            criteria: Vec::new(),
            artifacts: Vec::new(),
            failure_reason: None,
        }
    }

    #[test]
    fn validates_unique_acceptance_ids() {
        let task = EvalTask {
            id: "bug-1".to_string(),
            title: "fix bug".to_string(),
            category: EvalTaskCategory::BugFix,
            prompt: "fix it".to_string(),
            repository: None,
            base_revision: None,
            timeout_secs: Some(60),
            allowed_tools: vec!["read_file".to_string()],
            acceptance: vec![
                AcceptanceCriterion {
                    id: "tests".to_string(),
                    description: "tests pass".to_string(),
                    required: true,
                    evidence_hint: None,
                },
                AcceptanceCriterion {
                    id: "tests".to_string(),
                    description: "still pass".to_string(),
                    required: true,
                    evidence_hint: None,
                },
            ],
            tags: Vec::new(),
        };
        assert!(task.validate().is_err());
    }

    #[test]
    fn aggregates_core_agent_metrics() {
        let report = aggregate_outcomes(&[
            outcome(true, false, 10, 100),
            outcome(false, true, 20, 300),
        ]);
        assert_eq!(report.runs, 2);
        assert_eq!(report.successes, 1);
        assert_eq!(report.regressions, 1);
        assert_eq!(report.success_rate, 0.5);
        assert_eq!(report.regression_rate, 0.5);
        assert_eq!(report.average_tool_calls, 15.0);
        assert_eq!(report.average_tokens, 200.0);
    }
}
