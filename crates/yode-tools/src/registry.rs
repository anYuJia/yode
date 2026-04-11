use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tool::Tool;

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ToolInventory {
    pub total_count: usize,
    pub active_count: usize,
    pub deferred_count: usize,
    pub mcp_active_count: usize,
    pub mcp_deferred_count: usize,
    pub tool_search_enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolPoolPhase {
    Active,
    Deferred,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolOrigin {
    Builtin,
    Mcp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolPermissionState {
    Allow,
    Confirm,
    Deny,
}

impl ToolPermissionState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Confirm => "confirm",
            Self::Deny => "deny",
        }
    }

    pub fn visible_to_model(&self) -> bool {
        !matches!(self, Self::Deny)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPoolEntry {
    pub name: String,
    pub phase: ToolPoolPhase,
    pub origin: ToolOrigin,
    pub permission: ToolPermissionState,
    pub visible_to_model: bool,
    pub reason: String,
    pub matched_rule: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolPoolSnapshot {
    pub permission_mode: String,
    pub tool_search_enabled: bool,
    pub entries: Vec<ToolPoolEntry>,
}

impl ToolPoolSnapshot {
    fn count_matching(&self, predicate: impl Fn(&ToolPoolEntry) -> bool) -> usize {
        self.entries.iter().filter(|entry| predicate(entry)).count()
    }

    pub fn find_entry(&self, name: &str) -> Option<&ToolPoolEntry> {
        self.entries
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    pub fn active_visible_to_model(&self, name: &str) -> bool {
        self.find_entry(name)
            .is_some_and(|entry| entry.phase == ToolPoolPhase::Active && entry.visible_to_model)
    }

    pub fn visible_active_count(&self) -> usize {
        self.count_matching(|entry| entry.phase == ToolPoolPhase::Active && entry.visible_to_model)
    }

    pub fn hidden_active_count(&self) -> usize {
        self.count_matching(|entry| entry.phase == ToolPoolPhase::Active && !entry.visible_to_model)
    }

    pub fn visible_deferred_count(&self) -> usize {
        self.count_matching(|entry| {
            entry.phase == ToolPoolPhase::Deferred && entry.visible_to_model
        })
    }

    pub fn hidden_deferred_count(&self) -> usize {
        self.count_matching(|entry| {
            entry.phase == ToolPoolPhase::Deferred && !entry.visible_to_model
        })
    }

    pub fn visible_builtin_count(&self) -> usize {
        self.count_matching(|entry| entry.origin == ToolOrigin::Builtin && entry.visible_to_model)
    }

    pub fn visible_mcp_count(&self) -> usize {
        self.count_matching(|entry| entry.origin == ToolOrigin::Mcp && entry.visible_to_model)
    }

    pub fn confirm_count(&self) -> usize {
        self.count_matching(|entry| entry.permission == ToolPermissionState::Confirm)
    }

    pub fn deny_count(&self) -> usize {
        self.count_matching(|entry| entry.permission == ToolPermissionState::Deny)
    }

    pub fn hidden_tool_names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|entry| !entry.visible_to_model)
            .map(|entry| entry.name.as_str())
            .collect()
    }

    pub fn visible_deferred_tool_names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|entry| entry.phase == ToolPoolPhase::Deferred && entry.visible_to_model)
            .map(|entry| entry.name.as_str())
            .collect()
    }
}

/// Threshold: when total tool count exceeds this, deferred/lazy loading is enabled.
const TOOL_SEARCH_THRESHOLD: usize = 40;

pub struct ToolRegistry {
    /// Active tools sent to the LLM.
    tools: HashMap<String, Arc<dyn Tool>>,
    /// Deferred tools: known but not sent to LLM. Activated via tool_search.
    deferred: HashMap<String, Arc<dyn Tool>>,
    /// Whether tool search mode is enabled (auto or manual).
    tool_search_enabled: bool,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            deferred: HashMap::new(),
            tool_search_enabled: false,
        }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        tracing::debug!(tool_name = %name, "Registering tool");
        self.tools.insert(name, tool);
    }

    /// Register a tool as deferred (will not be sent to LLM until activated).
    pub fn register_deferred(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        tracing::debug!(tool_name = %name, "Registering deferred tool");
        self.deferred.insert(name, tool);
    }

    /// Move a deferred tool to the active set.
    pub fn activate_tool(&mut self, name: &str) -> bool {
        if let Some(tool) = self.deferred.remove(name) {
            self.tools.insert(name.to_string(), tool);
            true
        } else {
            false
        }
    }

    /// Enable or disable tool search mode.
    pub fn set_tool_search_enabled(&mut self, enabled: bool) {
        self.tool_search_enabled = enabled;
    }

    /// Check if tool search mode should be auto-enabled based on tool count.
    pub fn should_enable_tool_search(&self) -> bool {
        self.tools.len() + self.deferred.len() > TOOL_SEARCH_THRESHOLD
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools
            .get(name)
            .cloned()
            .or_else(|| self.deferred.get(name).cloned())
    }

    pub fn list(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }

    /// List deferred tools (name, tool).
    pub fn list_deferred(&self) -> Vec<(String, Arc<dyn Tool>)> {
        self.deferred
            .iter()
            .map(|(name, tool)| (name.clone(), Arc::clone(tool)))
            .collect()
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tool| ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                parameters: tool.parameters_schema(),
            })
            .collect()
    }

    /// Total number of tools (active + deferred).
    pub fn total_count(&self) -> usize {
        self.tools.len() + self.deferred.len()
    }

    pub fn inventory(&self) -> ToolInventory {
        ToolInventory {
            total_count: self.total_count(),
            active_count: self.tools.len(),
            deferred_count: self.deferred.len(),
            mcp_active_count: self
                .tools
                .keys()
                .filter(|name| name.starts_with("mcp__"))
                .count(),
            mcp_deferred_count: self
                .deferred
                .keys()
                .filter(|name| name.starts_with("mcp__"))
                .count(),
            tool_search_enabled: self.tool_search_enabled,
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
