use super::*;
use crate::permission::bash::auto_mode_bash_decision;
use crate::permission::tool_categories;

#[derive(Debug, Clone)]
struct AutoPermissionClassifierOutcome {
    action: PermissionAction,
    risk: Option<CommandRiskLevel>,
    reason: String,
    stage: &'static str,
}

struct AutoPermissionClassifier;

impl AutoPermissionClassifier {
    fn classify(
        manager: &PermissionManager,
        tool_name: &str,
        content: Option<&str>,
    ) -> Option<AutoPermissionClassifierOutcome> {
        if tool_name == "bash" {
            let command = content?;
            let (action, risk, reason) = auto_mode_bash_decision(command)?;
            return Some(AutoPermissionClassifierOutcome {
                action,
                risk: Some(risk),
                reason,
                stage: "bash_classifier",
            });
        }

        if manager.readonly_tools.iter().any(|tool| tool == tool_name) {
            return Some(AutoPermissionClassifierOutcome {
                action: PermissionAction::Allow,
                risk: None,
                reason: "Auto mode allows this read-only tool.".to_string(),
                stage: "readonly_allowlist",
            });
        }

        None
    }
}

impl PermissionManager {
    pub fn explain_with_content(
        &self,
        tool_name: &str,
        content: Option<&str>,
    ) -> PermissionExplanation {
        if self.mode == PermissionMode::Bypass {
            return PermissionExplanation {
                action: PermissionAction::Allow,
                reason: "Permission mode is bypass; all tools are allowed.".to_string(),
                mode: self.mode,
                classifier_risk: None,
                matched_rule: None,
                denial_count: 0,
                auto_skip_due_to_denials: false,
                precedence_chain: vec!["mode:bypass -> allow".to_string()],
            };
        }

        if self.mode == PermissionMode::Plan {
            if self.readonly_tools.iter().any(|tool| tool == tool_name) {
                return PermissionExplanation {
                    action: PermissionAction::Allow,
                    reason: "Plan mode allows this read-only tool.".to_string(),
                    mode: self.mode,
                    classifier_risk: None,
                    matched_rule: None,
                    denial_count: self.denial_tracker.denial_count(tool_name),
                    auto_skip_due_to_denials: false,
                    precedence_chain: vec!["mode:plan readonly -> allow".to_string()],
                };
            }
            return PermissionExplanation {
                action: PermissionAction::Deny,
                reason: format!(
                    "Plan mode blocks mutating tools. {}",
                    plan_mode_alternative_hint(tool_name)
                ),
                mode: self.mode,
                classifier_risk: None,
                matched_rule: None,
                denial_count: self.denial_tracker.denial_count(tool_name),
                auto_skip_due_to_denials: false,
                precedence_chain: vec!["mode:plan mutation -> deny".to_string()],
            };
        }

        if self.mode == PermissionMode::AcceptEdits
            && matches!(
                tool_name,
                "write_file" | "edit_file" | "multi_edit" | "notebook_edit"
            )
        {
            return PermissionExplanation {
                action: PermissionAction::Allow,
                reason: "Accept-edits mode auto-approves file modification tools.".to_string(),
                mode: self.mode,
                classifier_risk: None,
                matched_rule: None,
                denial_count: self.denial_tracker.denial_count(tool_name),
                auto_skip_due_to_denials: false,
                precedence_chain: vec!["mode:accept-edits write-tool -> allow".to_string()],
            };
        }

        if self.mode == PermissionMode::Auto {
            if let Some(outcome) = AutoPermissionClassifier::classify(self, tool_name, content) {
                let action_label = outcome.action.label().to_string();
                return PermissionExplanation {
                    action: outcome.action,
                    reason: outcome.reason,
                    mode: self.mode,
                    classifier_risk: outcome.risk,
                    matched_rule: None,
                    denial_count: self.denial_tracker.denial_count(tool_name),
                    auto_skip_due_to_denials: false,
                    precedence_chain: vec![format!(
                        "auto-classifier:{} -> {}{}",
                        outcome.stage,
                        action_label,
                        outcome
                            .risk
                            .map(|risk| format!(" ({:?})", risk))
                            .unwrap_or_default()
                    )],
                };
            }
        }

        if self.denial_tracker.should_auto_skip(tool_name) {
            return PermissionExplanation {
                action: PermissionAction::Deny,
                reason: format!(
                    "Recent denials for '{}' crossed the auto-skip threshold.",
                    tool_name
                ),
                mode: self.mode,
                classifier_risk: None,
                matched_rule: None,
                denial_count: self.denial_tracker.denial_count(tool_name),
                auto_skip_due_to_denials: true,
                precedence_chain: vec!["denial-tracker:auto-skip -> deny".to_string()],
            };
        }

        let mut matching_rules: Vec<&PermissionRule> = self
            .rules
            .iter()
            .filter(|rule| rule.matches(tool_name, content))
            .collect();
        matching_rules.sort_by(|a, b| b.source.cmp(&a.source));
        let precedence_chain = matching_rules
            .iter()
            .map(|rule| format_permission_rule(rule, tool_name))
            .collect::<Vec<_>>();

        if let Some(rule) = matching_rules.first() {
            let action = match rule.behavior {
                RuleBehavior::Allow => PermissionAction::Allow,
                RuleBehavior::Deny => PermissionAction::Deny,
                RuleBehavior::Ask => PermissionAction::Confirm,
            };
            return PermissionExplanation {
                action,
                reason: permission_rule_reason(rule, content),
                mode: self.mode,
                classifier_risk: None,
                matched_rule: Some(format!(
                    "{}:{}{}{}",
                    rule.tool_name,
                    match rule.behavior {
                        RuleBehavior::Allow => "allow",
                        RuleBehavior::Deny => "deny",
                        RuleBehavior::Ask => "ask",
                    },
                    rule.category
                        .as_ref()
                        .map(|category| format!(" [category={}]", category))
                        .unwrap_or_default(),
                    rule.pattern
                        .as_ref()
                        .map(|pattern| format!(" ({})", pattern))
                        .unwrap_or_default()
                )),
                denial_count: self.denial_tracker.denial_count(tool_name),
                auto_skip_due_to_denials: false,
                precedence_chain,
            };
        }

        let action = match tool_name {
            "bash" | "write_file" | "edit_file" | "multi_edit" | "notebook_edit" | "git_commit" => {
                PermissionAction::Confirm
            }
            _ => PermissionAction::Allow,
        };
        let action_label = action.label().to_string();
        PermissionExplanation {
            action,
            reason: "Fell back to the built-in default permission policy.".to_string(),
            mode: self.mode,
            classifier_risk: None,
            matched_rule: None,
            denial_count: self.denial_tracker.denial_count(tool_name),
            auto_skip_due_to_denials: false,
            precedence_chain: vec![format!(
                "builtin-default:{} -> {} / categories={} / fallback_stage=default_policy",
                tool_name,
                action_label,
                tool_categories(tool_name).join(",")
            )],
        }
    }
}

fn permission_rule_reason(rule: &PermissionRule, content: Option<&str>) -> String {
    let behavior = match rule.behavior {
        RuleBehavior::Allow => "allow",
        RuleBehavior::Deny => "deny",
        RuleBehavior::Ask => "ask",
    };
    match (&rule.pattern, content) {
        (Some(pattern), Some(command)) => format!(
            "Matched {} rule from {:?}{} because the command matched pattern '{}' against '{}'.",
            behavior,
            rule.source,
            rule.category
                .as_ref()
                .map(|category| format!(" on category '{}'", category))
                .unwrap_or_default(),
            pattern,
            command
        ),
        (Some(pattern), None) => format!(
            "Matched {} rule from {:?}{} with pattern '{}'.",
            behavior,
            rule.source,
            rule.category
                .as_ref()
                .map(|category| format!(" on category '{}'", category))
                .unwrap_or_default(),
            pattern
        ),
        (None, _) => format!(
            "Matched {} rule from {:?}{}.",
            behavior,
            rule.source,
            rule.category
                .as_ref()
                .map(|category| format!(" on category '{}'", category))
                .unwrap_or_default()
        ),
    }
}

fn format_permission_rule(rule: &PermissionRule, tool_name: &str) -> String {
    format!(
        "{:?}: {}{} -> {}{}{}",
        rule.source,
        rule.tool_name,
        rule.category
            .as_ref()
            .map(|category| format!(" [category={}]", category))
            .unwrap_or_default(),
        rule.behavior.label(),
        rule.pattern
            .as_ref()
            .map(|pattern| format!(" pattern={}", pattern))
            .unwrap_or_default(),
        if rule.tool_name == "*" {
            format!(" (applied to {})", tool_name)
        } else {
            String::new()
        }
    )
}

fn plan_mode_alternative_hint(tool_name: &str) -> &'static str {
    match tool_name {
        "write_file" | "edit_file" | "multi_edit" | "notebook_edit" => {
            "Use read_file / grep / project_map first to refine the plan before making edits."
        }
        "bash" => {
            "Use grep / glob / git_status / git_diff / project_map to gather evidence before mutating shell commands."
        }
        "git_commit" | "review_then_commit" | "review_pipeline" => {
            "Finish planning first, then exit plan mode before commit or review/ship pipelines."
        }
        "workflow_run_with_writes" => {
            "Use workflow_run dry-run or safe mode while planning; reserve write-capable workflows for execution mode."
        }
        "agent" | "coordinate_agents" => {
            "Prefer dry-run planning or read-only exploration until the execution plan is approved."
        }
        _ => "Switch to a read-only discovery step or exit plan mode before executing mutations.",
    }
}
