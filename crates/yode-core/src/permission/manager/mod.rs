mod explain;

use super::{
    CommandRiskLevel, DenialClusterView, DenialRecordView, DenialTracker, PermissionAction, PermissionMode,
    PermissionRule, RuleBehavior, RuleSource,
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
        let mut manager = Self::new(PermissionMode::Default);
        for tool in &require_confirmation {
            manager.rules.push(PermissionRule {
                source: RuleSource::UserConfig,
                behavior: RuleBehavior::Ask,
                tool_name: tool.clone(),
                pattern: None,
            });
        }
        manager
    }

    /// Create a permissive manager (bypass mode).
    pub fn permissive() -> Self {
        Self::new(PermissionMode::Bypass)
    }

    /// Create a strict manager that requires confirmation for dangerous tools.
    pub fn strict() -> Self {
        let mut manager = Self::new(PermissionMode::Default);
        for tool in &["bash", "write_file", "edit_file"] {
            manager.rules.push(PermissionRule {
                source: RuleSource::UserConfig,
                behavior: RuleBehavior::Ask,
                tool_name: tool.to_string(),
                pattern: None,
            });
        }
        manager
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
        self.rules.retain(|rule| rule.source != source);
    }

    /// Check if a tool is allowed to execute.
    pub fn check(&self, tool_name: &str) -> PermissionAction {
        self.check_with_content(tool_name, None)
    }

    /// Check with optional command content for pattern matching.
    pub fn check_with_content(&self, tool_name: &str, content: Option<&str>) -> PermissionAction {
        self.explain_with_content(tool_name, content).action
    }

    pub fn record_denial(&mut self, tool_name: &str) {
        self.denial_tracker.record_denial(tool_name);
    }

    pub fn record_success(&mut self, tool_name: &str) {
        self.denial_tracker.record_success(tool_name);
    }

    pub fn record_shell_prefix_denial(&mut self, content: Option<&str>) {
        if let Some(prefix) = content.and_then(crate::permission::bash::command_prefix) {
            self.denial_tracker.record_shell_prefix_denial(&prefix);
        }
    }

    pub fn recent_denials(&self, limit: usize) -> Vec<DenialRecordView> {
        self.denial_tracker.recent_entries(limit)
    }

    pub fn recent_denial_prefixes(&self, limit: usize) -> Vec<DenialClusterView> {
        self.denial_tracker.recent_shell_prefix_entries(limit)
    }

    pub fn safe_readonly_shell_prefixes(&self) -> &'static [&'static str] {
        crate::permission::bash::safe_readonly_prefixes()
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
            .filter(|rule| matches!(rule.behavior, RuleBehavior::Ask))
            .map(|rule| rule.tool_name.as_str())
            .collect();
        tools.sort();
        tools.dedup();
        tools
    }
}
