use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    Discover,
    Plan,
    Execute,
    Verify,
    Replan,
    Deliver,
    Completed,
    Failed,
    Cancelled,
}

impl RunPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunSignal {
    DiscoveryComplete,
    PlanReady,
    ExecutionComplete,
    VerificationPassed {
        required_total: u32,
        passed: u32,
    },
    VerificationFailed {
        required_total: u32,
        passed: u32,
        failed: u32,
        detail: String,
    },
    ReplanReady,
    DeliveryComplete,
    Fail {
        reason: String,
    },
    Cancel {
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunTransitionRecord {
    pub sequence: u64,
    pub at: DateTime<Utc>,
    pub from: RunPhase,
    pub to: RunPhase,
    pub signal: RunSignal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunControllerState {
    pub run_id: String,
    pub goal: String,
    pub phase: RunPhase,
    pub attempt: u32,
    pub replan_count: u32,
    pub max_replans: u32,
    #[serde(default)]
    pub last_failure: Option<String>,
    #[serde(default)]
    pub history: Vec<RunTransitionRecord>,
}

impl RunControllerState {
    pub fn new(run_id: impl Into<String>, goal: impl Into<String>, max_replans: u32) -> Self {
        Self {
            run_id: run_id.into(),
            goal: goal.into(),
            phase: RunPhase::Discover,
            attempt: 1,
            replan_count: 0,
            max_replans,
            last_failure: None,
            history: Vec::new(),
        }
    }

    pub fn apply(&mut self, signal: RunSignal) -> Result<RunTransitionRecord> {
        if self.phase.is_terminal() {
            return Err(anyhow!(
                "run '{}' is already terminal in phase {:?}",
                self.run_id,
                self.phase
            ));
        }

        let from = self.phase;
        let to = match (&self.phase, &signal) {
            (_, RunSignal::Fail { reason }) => {
                self.last_failure = Some(reason.clone());
                RunPhase::Failed
            }
            (_, RunSignal::Cancel { reason }) => {
                self.last_failure = reason.clone();
                RunPhase::Cancelled
            }
            (RunPhase::Discover, RunSignal::DiscoveryComplete) => RunPhase::Plan,
            (RunPhase::Plan, RunSignal::PlanReady) => RunPhase::Execute,
            (RunPhase::Execute, RunSignal::ExecutionComplete) => RunPhase::Verify,
            (
                RunPhase::Verify,
                RunSignal::VerificationPassed {
                    required_total,
                    passed,
                },
            ) => {
                if passed < required_total {
                    return Err(anyhow!(
                        "verification cannot pass: {passed}/{required_total} required criteria passed"
                    ));
                }
                RunPhase::Deliver
            }
            (
                RunPhase::Verify,
                RunSignal::VerificationFailed {
                    required_total,
                    passed,
                    failed,
                    detail,
                },
            ) => {
                self.last_failure = Some(format!(
                    "verification failed: {passed}/{required_total} passed, {failed} failed: {detail}"
                ));
                if self.replan_count < self.max_replans {
                    self.replan_count += 1;
                    self.attempt += 1;
                    RunPhase::Replan
                } else {
                    RunPhase::Failed
                }
            }
            (RunPhase::Replan, RunSignal::ReplanReady) => RunPhase::Execute,
            (RunPhase::Deliver, RunSignal::DeliveryComplete) => RunPhase::Completed,
            _ => {
                return Err(anyhow!(
                    "illegal run transition for '{}': {:?} + {:?}",
                    self.run_id,
                    self.phase,
                    signal
                ))
            }
        };

        self.phase = to;
        let record = RunTransitionRecord {
            sequence: self.history.len() as u64 + 1,
            at: Utc::now(),
            from,
            to,
            signal,
        };
        self.history.push(record.clone());
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_requires_verification_before_delivery() {
        let mut run = RunControllerState::new("run-1", "ship feature", 2);
        run.apply(RunSignal::DiscoveryComplete).unwrap();
        run.apply(RunSignal::PlanReady).unwrap();
        run.apply(RunSignal::ExecutionComplete).unwrap();
        run.apply(RunSignal::VerificationPassed {
            required_total: 3,
            passed: 3,
        })
        .unwrap();
        run.apply(RunSignal::DeliveryComplete).unwrap();

        assert_eq!(run.phase, RunPhase::Completed);
        assert_eq!(run.history.len(), 5);
        assert_eq!(run.replan_count, 0);
    }

    #[test]
    fn failed_verification_enters_replan_then_execute() {
        let mut run = RunControllerState::new("run-2", "fix bug", 2);
        run.apply(RunSignal::DiscoveryComplete).unwrap();
        run.apply(RunSignal::PlanReady).unwrap();
        run.apply(RunSignal::ExecutionComplete).unwrap();
        run.apply(RunSignal::VerificationFailed {
            required_total: 2,
            passed: 1,
            failed: 1,
            detail: "regression test failed".to_string(),
        })
        .unwrap();
        assert_eq!(run.phase, RunPhase::Replan);
        assert_eq!(run.replan_count, 1);
        assert_eq!(run.attempt, 2);

        run.apply(RunSignal::ReplanReady).unwrap();
        assert_eq!(run.phase, RunPhase::Execute);
    }

    #[test]
    fn exhausted_replan_budget_fails_closed() {
        let mut run = RunControllerState::new("run-3", "fix bug", 0);
        run.apply(RunSignal::DiscoveryComplete).unwrap();
        run.apply(RunSignal::PlanReady).unwrap();
        run.apply(RunSignal::ExecutionComplete).unwrap();
        run.apply(RunSignal::VerificationFailed {
            required_total: 1,
            passed: 0,
            failed: 1,
            detail: "test failed".to_string(),
        })
        .unwrap();

        assert_eq!(run.phase, RunPhase::Failed);
        assert!(run.last_failure.as_deref().unwrap().contains("test failed"));
    }

    #[test]
    fn verification_pass_rejects_missing_required_criteria() {
        let mut run = RunControllerState::new("run-4", "fix bug", 1);
        run.apply(RunSignal::DiscoveryComplete).unwrap();
        run.apply(RunSignal::PlanReady).unwrap();
        run.apply(RunSignal::ExecutionComplete).unwrap();
        assert!(run
            .apply(RunSignal::VerificationPassed {
                required_total: 2,
                passed: 1,
            })
            .is_err());
        assert_eq!(run.phase, RunPhase::Verify);
    }

    #[test]
    fn terminal_runs_cannot_reopen() {
        let mut run = RunControllerState::new("run-5", "cancel", 1);
        run.apply(RunSignal::Cancel { reason: None }).unwrap();
        assert!(run.apply(RunSignal::DiscoveryComplete).is_err());
        assert_eq!(run.phase, RunPhase::Cancelled);
    }
}
