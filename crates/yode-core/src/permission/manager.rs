use super::classifier::bash_risk_rationale;
use super::{
    CommandClassifier, CommandRiskLevel, DenialRecordView, DenialTracker, PermissionAction,
    PermissionMode, PermissionRule, RuleBehavior, RuleSource,
};

#[derive(Debug, Clone)]
pub struct PermissionExplanation {
    pub action: PermissionAction,
    pub reason: String,
    pub mode: PermissionMode,
    pub classifier_risk: Option<CommandRiskLevel>,
    pub matched_rule: Option<String>,
    pub denial_count: u32,
    pub auto_skip_due_to_denials: bool,
}

/// Manages permissions for tool execution with modes, rules, and tracking.
#[derive(Debug)]
pub struct PermissionManager {
    mode: PermissionMode,
    rules: Vec<PermissionRule>,
    denial_tracker: DenialTracker,
    /// Read-only tool names that are always allowed in plan mode
    readonly_tools: Vec<String>,
}

impl PermissionManager {
    pub fn new(mode: PermissionMode) -> Self {
        Self {
            mode,
            rules: Vec::new(),
            denial_tracker: DenialTracker::new(),
            readonly_tools: vec![
                "read_file".into(),
                "glob".into(),
                "grep".into(),
                "ls".into(),
                "git_status".into(),
                "git_log".into(),
                "git_diff".into(),
                "project_map".into(),
                "tool_search".into(),
                "web_search".into(),
                "web_fetch".into(),
                "lsp".into(),
                "mcp_list_resources".into(),
                "mcp_read_resource".into(),
            ],
        }
    }

    /// Create from legacy confirmation list (backwards compatible).
    pub fn from_confirmation_list(require_confirmation: Vec<String>) -> Self {
        let mut mgr = Self::new(PermissionMode::Default);
        for tool in &require_confirmation {
            mgr.rules.push(PermissionRule {
                source: RuleSource::UserConfig,
                behavior: RuleBehavior::Ask,
                tool_name: tool.clone(),
                pattern: None,
            });
        }
        mgr
    }

    /// Create a permissive manager (bypass mode).
    pub fn permissive() -> Self {
        Self::new(PermissionMode::Bypass)
    }

    /// Create a strict manager that requires confirmation for dangerous tools.
    pub fn strict() -> Self {
        let mut mgr = Self::new(PermissionMode::Default);
        for tool in &["bash", "write_file", "edit_file"] {
            mgr.rules.push(PermissionRule {
                source: RuleSource::UserConfig,
                behavior: RuleBehavior::Ask,
                tool_name: tool.to_string(),
                pattern: None,
            });
        }
        mgr
    }

    pub fn mode(&self) -> PermissionMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: PermissionMode) {
        self.mode = mode;
    }

    pub fn add_rule(&mut self, rule: PermissionRule) {
        self.rules.push(rule);
    }

    pub fn add_rules(&mut self, rules: Vec<PermissionRule>) {
        self.rules.extend(rules);
    }

    /// Clear all rules from a specific source.
    pub fn clear_rules(&mut self, source: RuleSource) {
        self.rules.retain(|r| r.source != source);
    }

    /// Check if a tool is allowed to execute.
    pub fn check(&self, tool_name: &str) -> PermissionAction {
        self.check_with_content(tool_name, None)
    }

    /// Check with optional command content for pattern matching.
    pub fn check_with_content(&self, tool_name: &str, content: Option<&str>) -> PermissionAction {
        self.explain_with_content(tool_name, content).action
    }

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
            if self.readonly_tools.iter().any(|t| t == tool_name) {
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
            if let Some(cmd) = content {
                let risk = CommandClassifier::classify(cmd);
                match risk {
                    CommandRiskLevel::Safe => {
                        return PermissionExplanation {
                            action: PermissionAction::Allow,
                            reason: format!(
                                "Auto mode classifier marked this bash command as safe. {}",
                                bash_risk_rationale(cmd, risk)
                            ),
                            mode: self.mode,
                            classifier_risk: Some(risk),
                            matched_rule: None,
                            denial_count: self.denial_tracker.denial_count(tool_name),
                            auto_skip_due_to_denials: false,
                        };
                    }
                    CommandRiskLevel::Destructive => {
                        return PermissionExplanation {
                            action: PermissionAction::Deny,
                            reason: format!(
                                "Auto mode classifier marked this bash command as destructive. {}",
                                bash_risk_rationale(cmd, risk)
                            ),
                            mode: self.mode,
                            classifier_risk: Some(risk),
                            matched_rule: None,
                            denial_count: self.denial_tracker.denial_count(tool_name),
                            auto_skip_due_to_denials: false,
                        };
                    }
                    CommandRiskLevel::PotentiallyRisky => {
                        return PermissionExplanation {
                            action: PermissionAction::Confirm,
                            reason: format!(
                                "Auto mode classifier marked this bash command as potentially risky. {}",
                                bash_risk_rationale(cmd, risk)
                            ),
                            mode: self.mode,
                            classifier_risk: Some(risk),
                            matched_rule: None,
                            denial_count: self.denial_tracker.denial_count(tool_name),
                            auto_skip_due_to_denials: false,
                        };
                    }
                    CommandRiskLevel::Unknown => {}
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
            .filter(|r| r.matches(tool_name, content))
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

        if self.mode == PermissionMode::Auto && self.readonly_tools.iter().any(|t| t == tool_name) {
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
            "bash" | "write_file" | "edit_file" | "multi_edit" | "notebook_edit"
            | "git_commit" => PermissionAction::Confirm,
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

    pub fn record_denial(&mut self, tool_name: &str) {
        self.denial_tracker.record_denial(tool_name);
    }

    pub fn record_success(&mut self, tool_name: &str) {
        self.denial_tracker.record_success(tool_name);
    }

    pub fn recent_denials(&self, limit: usize) -> Vec<DenialRecordView> {
        self.denial_tracker.recent_entries(limit)
    }

    pub fn rules_snapshot(&self) -> Vec<PermissionRule> {
        self.rules.clone()
    }

    pub fn allow(&mut self, tool_name: &str) {
        self.rules.push(PermissionRule {
            source: RuleSource::Session,
            behavior: RuleBehavior::Allow,
            tool_name: tool_name.to_string(),
            pattern: None,
        });
    }

    pub fn deny(&mut self, tool_name: &str) {
        self.rules.push(PermissionRule {
            source: RuleSource::Session,
            behavior: RuleBehavior::Deny,
            tool_name: tool_name.to_string(),
            pattern: None,
        });
    }

    pub fn reset(&mut self, _defaults: Vec<String>) {
        self.clear_rules(RuleSource::Session);
    }

    pub fn confirmable_tools(&self) -> Vec<&str> {
        let mut tools: Vec<&str> = self
            .rules
            .iter()
            .filter(|r| matches!(r.behavior, RuleBehavior::Ask))
            .map(|r| r.tool_name.as_str())
            .collect();
        tools.sort();
        tools.dedup();
        tools
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
