mod explain;

use std::collections::HashMap;

use super::{
    CommandRiskLevel, CommandSemanticCategory, DenialClusterView, DenialRecordView, DenialTracker,
    PermissionAction, PermissionMode, PermissionRule, RuleBehavior, RuleSource,
};

#[derive(Debug, Clone)]
pub struct PermissionExplanation {
    pub action: PermissionAction,
    pub reason: String,
    pub mode: PermissionMode,
    pub classifier_risk: Option<CommandRiskLevel>,
    pub semantic_category: Option<CommandSemanticCategory>,
    pub semantic_segment: Option<String>,
    pub matched_rule: Option<String>,
    pub denial_count: u32,
    pub auto_skip_due_to_denials: bool,
    pub precedence_chain: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PermissionSourceView {
    pub source: RuleSource,
    pub path: Option<String>,
    pub default_mode: Option<String>,
    pub rules: Vec<PermissionRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionConflictView {
    pub higher_source: RuleSource,
    pub lower_source: RuleSource,
    pub tool_name: String,
    pub category: Option<String>,
    pub pattern: Option<String>,
    pub higher_behavior: RuleBehavior,
    pub lower_behavior: RuleBehavior,
}

/// Manages permissions for tool execution with modes, rules, and tracking.
#[derive(Debug)]
pub struct PermissionManager {
    mode: PermissionMode,
    rules: Vec<PermissionRule>,
    denial_tracker: DenialTracker,
    confirmation_prefix_counts: HashMap<String, u32>,
    source_views: Vec<PermissionSourceView>,
    /// Read-only tool names that are always allowed in plan mode
    readonly_tools: Vec<String>,
}

impl PermissionManager {
    pub fn new(mode: PermissionMode) -> Self {
        Self {
            mode,
            rules: Vec::new(),
            denial_tracker: DenialTracker::new(),
            confirmation_prefix_counts: HashMap::new(),
            source_views: Vec::new(),
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
                "list_mcp_resources".into(),
                "list_mcp_resource_templates".into(),
                "read_mcp_resource".into(),
                "request_user_input".into(),
            ],
        }
    }

    /// Create from legacy confirmation list (backwards compatible).
    pub fn from_confirmation_list(require_confirmation: Vec<String>) -> Self {
        let mut manager = Self::new(PermissionMode::Default);
        for tool in &require_confirmation {
            for tool_name in legacy_confirmation_tool_names(tool) {
                manager.rules.push(PermissionRule {
                    source: RuleSource::UserConfig,
                    behavior: RuleBehavior::Ask,
                    tool_name,
                    category: None,
                    pattern: None,
                    description: None,
                });
            }
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
                category: None,
                pattern: None,
                description: None,
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

    pub fn remove_rule(
        &mut self,
        source: RuleSource,
        behavior: RuleBehavior,
        tool_name: &str,
        pattern: Option<&str>,
    ) -> usize {
        let before = self.rules.len();
        self.rules.retain(|rule| {
            !(rule.source == source
                && rule.behavior == behavior
                && rule.tool_name == tool_name
                && rule.pattern.as_deref() == pattern)
        });
        before.saturating_sub(self.rules.len())
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
        if let Some(prefix) = content.and_then(crate::permission::shell::command_prefix) {
            self.denial_tracker.record_shell_prefix_denial(&prefix);
        }
    }

    pub fn record_confirmation_request(&mut self, tool_name: &str, content: Option<&str>) {
        if !matches!(tool_name, "bash" | "exec_command" | "shell_command") {
            return;
        }
        if let Some(prefix) = content.and_then(crate::permission::shell::command_prefix) {
            *self.confirmation_prefix_counts.entry(prefix).or_insert(0) += 1;
        }
    }

    pub fn recent_denials(&self, limit: usize) -> Vec<DenialRecordView> {
        self.denial_tracker.recent_entries(limit)
    }

    pub fn recent_denial_prefixes(&self, limit: usize) -> Vec<DenialClusterView> {
        self.denial_tracker.recent_shell_prefix_entries(limit)
    }

    pub fn safe_readonly_shell_prefixes(&self) -> &'static [&'static str] {
        crate::permission::shell::safe_readonly_prefixes()
    }

    pub fn confirmation_rule_suggestions(&self, min_count: u32) -> Vec<String> {
        let safe_prefixes = self.safe_readonly_shell_prefixes();
        let mut suggestions = self
            .confirmation_prefix_counts
            .iter()
            .filter(|(_, count)| **count >= min_count)
            .map(|(prefix, count)| {
                if safe_prefixes.iter().any(|item| item == prefix) {
                    format!(
                        "{} x{} -> consider adding an allow rule like `{{ tool = \"bash\", pattern = \"{}*\" }}` to always_allow",
                        prefix, count, prefix
                    )
                } else {
                    format!(
                        "{} x{} -> consider adding a scoped bash rule if this confirmation is expected repeatedly",
                        prefix, count
                    )
                }
            })
            .collect::<Vec<_>>();
        suggestions.sort();
        suggestions
    }

    pub fn rules_snapshot(&self) -> Vec<PermissionRule> {
        self.rules.clone()
    }

    pub fn set_source_views(&mut self, views: Vec<PermissionSourceView>) {
        self.source_views = views;
    }

    pub fn source_views_snapshot(&self) -> Vec<PermissionSourceView> {
        self.source_views.clone()
    }

    pub fn conflict_views_snapshot(&self) -> Vec<PermissionConflictView> {
        let mut conflicts = Vec::new();
        for higher in &self.rules {
            for lower in &self.rules {
                if higher.source <= lower.source {
                    continue;
                }
                if !rules_overlap(higher, lower) || higher.behavior == lower.behavior {
                    continue;
                }
                conflicts.push(PermissionConflictView {
                    higher_source: higher.source,
                    lower_source: lower.source,
                    tool_name: higher.tool_name.clone(),
                    category: higher.category.clone().or_else(|| lower.category.clone()),
                    pattern: higher.pattern.clone().or_else(|| lower.pattern.clone()),
                    higher_behavior: higher.behavior.clone(),
                    lower_behavior: lower.behavior.clone(),
                });
            }
        }
        conflicts.sort_by(|a, b| {
            (
                a.tool_name.as_str(),
                a.category.as_deref().unwrap_or(""),
                a.pattern.as_deref().unwrap_or(""),
                a.lower_source,
                a.higher_source,
            )
                .cmp(&(
                    b.tool_name.as_str(),
                    b.category.as_deref().unwrap_or(""),
                    b.pattern.as_deref().unwrap_or(""),
                    b.lower_source,
                    b.higher_source,
                ))
        });
        conflicts
    }

    pub fn allow(&mut self, tool_name: &str) {
        self.rules.push(PermissionRule {
            source: RuleSource::Session,
            behavior: RuleBehavior::Allow,
            tool_name: tool_name.to_string(),
            category: None,
            pattern: None,
            description: None,
        });
    }

    pub fn allow_category(&mut self, category: &str) {
        self.rules.push(PermissionRule {
            source: RuleSource::Session,
            behavior: RuleBehavior::Allow,
            tool_name: "*".to_string(),
            category: Some(category.to_string()),
            pattern: None,
            description: Some(format!("session allow for category {}", category)),
        });
    }

    pub fn deny(&mut self, tool_name: &str) {
        self.rules.push(PermissionRule {
            source: RuleSource::Session,
            behavior: RuleBehavior::Deny,
            tool_name: tool_name.to_string(),
            category: None,
            pattern: None,
            description: None,
        });
    }

    pub fn deny_category(&mut self, category: &str) {
        self.rules.push(PermissionRule {
            source: RuleSource::Session,
            behavior: RuleBehavior::Deny,
            tool_name: "*".to_string(),
            category: Some(category.to_string()),
            pattern: None,
            description: Some(format!("session deny for category {}", category)),
        });
    }

    pub fn ask_category(&mut self, category: &str) {
        self.rules.push(PermissionRule {
            source: RuleSource::Session,
            behavior: RuleBehavior::Ask,
            tool_name: "*".to_string(),
            category: Some(category.to_string()),
            pattern: None,
            description: Some(format!("session ask for category {}", category)),
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

fn legacy_confirmation_tool_names(tool: &str) -> Vec<String> {
    if tool.eq_ignore_ascii_case("bash") {
        vec![
            "bash".to_string(),
            "exec_command".to_string(),
            "shell_command".to_string(),
        ]
    } else {
        vec![tool.to_string()]
    }
}

fn rules_overlap(higher: &PermissionRule, lower: &PermissionRule) -> bool {
    let tool_matches =
        higher.tool_name == lower.tool_name || higher.tool_name == "*" || lower.tool_name == "*";
    let category_matches =
        higher.category.is_none() || lower.category.is_none() || higher.category == lower.category;
    let pattern_matches =
        higher.pattern.is_none() || lower.pattern.is_none() || higher.pattern == lower.pattern;
    tool_matches && category_matches && pattern_matches
}
