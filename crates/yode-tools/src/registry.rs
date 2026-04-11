use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, RwLock,
};

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
    pub tool_search_reason: Option<String>,
    pub activation_count: usize,
    pub last_activated_tool: Option<String>,
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
    pub tool_search_reason: Option<String>,
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
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
    /// Deferred tools: known but not sent to LLM. Activated via tool_search.
    deferred: RwLock<HashMap<String, Arc<dyn Tool>>>,
    /// Whether tool search mode is enabled (auto or manual).
    tool_search_enabled: AtomicBool,
    /// Why tool search is enabled or disabled for the current session.
    tool_search_reason: RwLock<Option<String>>,
    /// Number of deferred tools activated during the current session.
    activation_count: AtomicUsize,
    /// Most recent deferred tool activated into the live pool.
    last_activated_tool: RwLock<Option<String>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            deferred: RwLock::new(HashMap::new()),
            tool_search_enabled: AtomicBool::new(false),
            tool_search_reason: RwLock::new(None),
            activation_count: AtomicUsize::new(0),
            last_activated_tool: RwLock::new(None),
        }
    }

    pub fn register(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        tracing::debug!(tool_name = %name, "Registering tool");
        self.tools.write().unwrap().insert(name, tool);
    }

    /// Register a tool as deferred (will not be sent to LLM until activated).
    pub fn register_deferred(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        tracing::debug!(tool_name = %name, "Registering deferred tool");
        self.deferred.write().unwrap().insert(name, tool);
    }

    /// Move a deferred tool to the active set.
    pub fn activate_tool(&self, name: &str) -> bool {
        let tool = self.deferred.write().unwrap().remove(name);
        if let Some(tool) = tool {
            self.tools.write().unwrap().insert(name.to_string(), tool);
            self.activation_count.fetch_add(1, Ordering::Relaxed);
            *self.last_activated_tool.write().unwrap() = Some(name.to_string());
            true
        } else {
            false
        }
    }

    /// Enable or disable tool search mode.
    pub fn set_tool_search_enabled(&self, enabled: bool) {
        self.tool_search_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn set_tool_search_state(&self, enabled: bool, reason: impl Into<String>) {
        self.tool_search_enabled.store(enabled, Ordering::Relaxed);
        *self.tool_search_reason.write().unwrap() = Some(reason.into());
    }

    /// Check if tool search mode should be auto-enabled based on tool count.
    pub fn should_enable_tool_search(&self) -> bool {
        self.should_enable_tool_search_with_additional(0)
    }

    pub fn should_enable_tool_search_with_additional(&self, additional_tools: usize) -> bool {
        self.total_count() + additional_tools > TOOL_SEARCH_THRESHOLD
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools
            .read()
            .unwrap()
            .get(name)
            .cloned()
            .or_else(|| self.deferred.read().unwrap().get(name).cloned())
    }

    pub fn list(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.read().unwrap().values().cloned().collect()
    }

    /// List deferred tools (name, tool).
    pub fn list_deferred(&self) -> Vec<(String, Arc<dyn Tool>)> {
        self.deferred
            .read()
            .unwrap()
            .iter()
            .map(|(name, tool)| (name.clone(), Arc::clone(tool)))
            .collect()
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .read()
            .unwrap()
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
        self.tools.read().unwrap().len() + self.deferred.read().unwrap().len()
    }

    pub fn inventory(&self) -> ToolInventory {
        let tools = self.tools.read().unwrap();
        let deferred = self.deferred.read().unwrap();
        ToolInventory {
            total_count: tools.len() + deferred.len(),
            active_count: tools.len(),
            deferred_count: deferred.len(),
            mcp_active_count: tools
                .keys()
                .filter(|name| name.starts_with("mcp__"))
                .count(),
            mcp_deferred_count: deferred
                .keys()
                .filter(|name| name.starts_with("mcp__"))
                .count(),
            tool_search_enabled: self.tool_search_enabled.load(Ordering::Relaxed),
            tool_search_reason: self.tool_search_reason.read().unwrap().clone(),
            activation_count: self.activation_count.load(Ordering::Relaxed),
            last_activated_tool: self.last_activated_tool.read().unwrap().clone(),
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
