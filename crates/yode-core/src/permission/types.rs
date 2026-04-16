use serde::{Deserialize, Serialize};

/// Permission modes control how tool execution is authorized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    /// Default: dangerous tools require confirmation
    #[default]
    Default,
    /// Plan mode: only read-only tools allowed, no mutations
    Plan,
    /// Auto: use command classifier to auto-approve safe operations
    Auto,
    /// Accept edits: auto-approve file edits, still confirm bash
    AcceptEdits,
    /// Bypass: skip all permission checks (dangerous)
    Bypass,
}

impl std::fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => write!(f, "default"),
            Self::Plan => write!(f, "plan"),
            Self::Auto => write!(f, "auto"),
            Self::AcceptEdits => write!(f, "accept-edits"),
            Self::Bypass => write!(f, "bypass"),
        }
    }
}

impl std::str::FromStr for PermissionMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "default" => Ok(Self::Default),
            "plan" => Ok(Self::Plan),
            "auto" => Ok(Self::Auto),
            "accept-edits" | "acceptedits" | "accept_edits" => Ok(Self::AcceptEdits),
            "bypass" | "dont-ask" | "dontask" | "don't-ask" => Ok(Self::Bypass),
            _ => Err(format!(
                "Unknown permission mode: {s}. Valid: default, plan, auto, accept-edits, bypass"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionAction {
    Allow,
    Confirm,
    Deny,
}

impl PermissionAction {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Confirm => "confirm",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RuleSource {
    /// User-level config (~/.yode/config.toml)
    UserConfig = 0,
    /// Project-level config (.yode/config.toml)
    ProjectConfig = 1,
    /// Local developer override (.yode/config.local.toml)
    LocalConfig = 2,
    /// Session-level rules (dynamic)
    Session = 3,
    /// Managed/enterprise policy (~/.yode/managed-config.toml)
    ManagedConfig = 4,
    /// CLI arguments (highest priority)
    CliArg = 5,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleBehavior {
    Allow,
    Deny,
    Ask,
}

impl RuleBehavior {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Ask => "ask",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub source: RuleSource,
    pub behavior: RuleBehavior,
    pub tool_name: String,
    /// Optional tool category match (e.g. read, write, shell, agent, mcp)
    pub category: Option<String>,
    /// Optional glob/pattern for command content matching
    pub pattern: Option<String>,
    /// Optional operator-facing description for the rule.
    pub description: Option<String>,
}

impl PermissionRule {
    /// Check if this rule matches a given tool name and optional command content.
    pub(crate) fn matches(&self, tool_name: &str, content: Option<&str>) -> bool {
        let category_match = self
            .category
            .as_deref()
            .is_some_and(|category| tool_categories(tool_name).iter().any(|item| *item == category));
        let tool_match = if self.tool_name == "*" {
            self.category.is_none()
        } else {
            self.tool_name.to_lowercase() == tool_name.to_lowercase()
        };
        if !(tool_match || category_match) {
            return false;
        }
        match (&self.pattern, content) {
            (None, _) => true,
            (Some(pattern), Some(content)) => glob_match(pattern, content),
            (Some(_), None) => false,
        }
    }
}

/// Simple glob matching (supports * as wildcard).
pub(crate) fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let text = text.to_lowercase();

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return text == pattern;
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(found) = text[pos..].find(part) {
            if i == 0 && found != 0 {
                return false;
            }
            pos += found + part.len();
        } else {
            return false;
        }
    }

    if !pattern.ends_with('*') {
        return pos == text.len();
    }
    true
}

pub fn tool_categories(tool_name: &str) -> Vec<&'static str> {
    let mut categories = Vec::new();
    match tool_name {
        "read_file" | "glob" | "grep" | "ls" | "git_status" | "git_log" | "git_diff"
        | "project_map" | "tool_search" | "web_search" | "web_fetch" | "lsp"
        | "mcp_list_resources" | "mcp_read_resource" | "task_output" | "cron_list" => {
            categories.push("read");
        }
        "write_file" | "edit_file" | "multi_edit" | "notebook_edit" | "git_commit"
        | "cron_create" | "cron_delete" | "enter_worktree" | "exit_worktree" => {
            categories.push("write");
        }
        "bash" => categories.push("shell"),
        "agent" | "coordinate_agents" | "verification_agent" => {
            categories.push("agent");
            categories.push("team");
        }
        "team_create" | "send_message" | "team_monitor" => categories.push("team"),
        "remote_queue_dispatch" | "remote_queue_result" | "remote_transport_control" => {
            categories.push("remote");
        }
        name if name.starts_with("mcp__") => categories.push("mcp"),
        name if name.starts_with("workflow") => categories.push("workflow"),
        _ => {}
    }
    if matches!(tool_name, "task_output" | "cron_create" | "cron_list" | "cron_delete") {
        categories.push("background");
    }
    if matches!(
        tool_name,
        "remote_queue_dispatch" | "remote_queue_result" | "remote_transport_control"
    ) {
        categories.push("background");
    }
    if categories.is_empty() {
        categories.push("general");
    }
    categories
}
