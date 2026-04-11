use super::*;
use crate::permission::bash::auto_mode_bash_decision;

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
            };
        }

        if self.mode == PermissionMode::Auto && tool_name == "bash" {
            if let Some(command) = content {
                if let Some((action, risk, reason)) = auto_mode_bash_decision(command) {
                    return PermissionExplanation {
                        action,
                        reason,
                        mode: self.mode,
                        classifier_risk: Some(risk),
                        matched_rule: None,
                        denial_count: self.denial_tracker.denial_count(tool_name),
                        auto_skip_due_to_denials: false,
                    };
                }
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
            };
        }

        let mut matching_rules: Vec<&PermissionRule> = self
            .rules
            .iter()
            .filter(|rule| rule.matches(tool_name, content))
            .collect();
        matching_rules.sort_by(|a, b| b.source.cmp(&a.source));

        if let Some(rule) = matching_rules.first() {
            let action = match rule.behavior {
                RuleBehavior::Allow => PermissionAction::Allow,
                RuleBehavior::Deny => PermissionAction::Deny,
                RuleBehavior::Ask => PermissionAction::Confirm,
            };
            return PermissionExplanation {
                action,
                reason: format!(
                    "Matched {} rule from {:?}.",
                    match rule.behavior {
                        RuleBehavior::Allow => "allow",
                        RuleBehavior::Deny => "deny",
                        RuleBehavior::Ask => "ask",
                    },
                    rule.source
                ),
                mode: self.mode,
                classifier_risk: None,
                matched_rule: Some(format!(
                    "{}:{}{}",
                    rule.tool_name,
                    match rule.behavior {
                        RuleBehavior::Allow => "allow",
                        RuleBehavior::Deny => "deny",
                        RuleBehavior::Ask => "ask",
                    },
                    rule.pattern
                        .as_ref()
                        .map(|pattern| format!(" ({})", pattern))
                        .unwrap_or_default()
                )),
                denial_count: self.denial_tracker.denial_count(tool_name),
                auto_skip_due_to_denials: false,
            };
        }

        if self.mode == PermissionMode::Auto && self.readonly_tools.iter().any(|tool| tool == tool_name) {
            return PermissionExplanation {
                action: PermissionAction::Allow,
                reason: "Auto mode allows this read-only tool.".to_string(),
                mode: self.mode,
                classifier_risk: None,
                matched_rule: None,
                denial_count: self.denial_tracker.denial_count(tool_name),
                auto_skip_due_to_denials: false,
            };
        }

        let action = match tool_name {
            "bash" | "write_file" | "edit_file" | "multi_edit" | "notebook_edit" | "git_commit" => {
                PermissionAction::Confirm
            }
            _ => PermissionAction::Allow,
        };
        PermissionExplanation {
            action,
            reason: "Fell back to the built-in default permission policy.".to_string(),
            mode: self.mode,
            classifier_risk: None,
            matched_rule: None,
            denial_count: self.denial_tracker.denial_count(tool_name),
            auto_skip_due_to_denials: false,
        }
    }
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
