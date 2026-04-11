pub(crate) mod bash;
mod classifier;
mod config;
mod denial_tracker;
mod manager;
mod types;

pub(crate) use classifier::bash_risk_rationale;
pub use classifier::{CommandClassifier, CommandRiskLevel};
pub use config::{PermissionConfig, PermissionRuleConfig};
pub use denial_tracker::{DenialClusterView, DenialRecordView, DenialTracker};
pub use manager::{PermissionExplanation, PermissionManager};
pub use types::{PermissionAction, PermissionMode, PermissionRule, RuleBehavior, RuleSource};

#[cfg(test)]
mod tests;
