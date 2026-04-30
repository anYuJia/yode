use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, RwLock, RwLockReadGuard, RwLockWriteGuard,
};

use serde::{Deserialize, Serialize};

use crate::tool::{Tool, ToolDefinition};

#[derive(Debug, Clone, Default)]
pub struct ToolInventory {
    pub total_count: usize,
    pub active_count: usize,
    pub deferred_count: usize,
    pub mcp_active_count: usize,
    pub mcp_deferred_count: usize,
    pub tool_search_enabled: bool,
    pub tool_search_reason: Option<String>,
    pub duplicate_registration_count: usize,
    pub duplicate_tool_names: Vec<String>,
    pub activation_count: usize,
    pub last_activated_tool: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateToolRegistration {
    pub name: String,
    pub original_phase: ToolPoolPhase,
    pub duplicate_phase: ToolPoolPhase,
    pub attempts: usize,
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
    /// Duplicate registration attempts keyed by tool name.
    duplicate_registrations: RwLock<HashMap<String, DuplicateToolRegistration>>,
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
            duplicate_registrations: RwLock::new(HashMap::new()),
            activation_count: AtomicUsize::new(0),
            last_activated_tool: RwLock::new(None),
        }
    }

    pub fn register(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        if self.record_duplicate_if_present(&name, ToolPoolPhase::Active) {
            return;
        }
        tracing::debug!(tool_name = %name, "Registering tool");
        write_lock(&self.tools, "tools").insert(name, tool);
    }

    /// Register a tool as deferred (will not be sent to LLM until activated).
    pub fn register_deferred(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        if self.record_duplicate_if_present(&name, ToolPoolPhase::Deferred) {
            return;
        }
        tracing::debug!(tool_name = %name, "Registering deferred tool");
        write_lock(&self.deferred, "deferred").insert(name, tool);
    }

    /// Move a deferred tool to the active set.
    pub fn activate_tool(&self, name: &str) -> bool {
        let tool = write_lock(&self.deferred, "deferred").remove(name);
        if let Some(tool) = tool {
            write_lock(&self.tools, "tools").insert(name.to_string(), tool);
            self.activation_count.fetch_add(1, Ordering::Relaxed);
            *write_lock(&self.last_activated_tool, "last_activated_tool") = Some(name.to_string());
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
        *write_lock(&self.tool_search_reason, "tool_search_reason") = Some(reason.into());
    }

    /// Check if tool search mode should be auto-enabled based on tool count.
    pub fn should_enable_tool_search(&self) -> bool {
        self.should_enable_tool_search_with_additional(0)
    }

    pub fn should_enable_tool_search_with_additional(&self, additional_tools: usize) -> bool {
        self.total_count() + additional_tools > TOOL_SEARCH_THRESHOLD
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        read_lock(&self.tools, "tools")
            .get(name)
            .cloned()
            .or_else(|| read_lock(&self.deferred, "deferred").get(name).cloned())
    }

    pub fn list(&self) -> Vec<Arc<dyn Tool>> {
        read_lock(&self.tools, "tools").values().cloned().collect()
    }

    /// List deferred tools (name, tool).
    pub fn list_deferred(&self) -> Vec<(String, Arc<dyn Tool>)> {
        self.deferred
            .read()
            .unwrap_or_else(|poisoned| {
                tracing::warn!(lock = "deferred", "Recovering poisoned tool registry lock");
                poisoned.into_inner()
            })
            .iter()
            .map(|(name, tool)| (name.clone(), Arc::clone(tool)))
            .collect()
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .read()
            .unwrap_or_else(|poisoned| {
                tracing::warn!(lock = "tools", "Recovering poisoned tool registry lock");
                poisoned.into_inner()
            })
            .values()
            .map(|tool| tool.definition())
            .collect()
    }

    /// Total number of tools (active + deferred).
    pub fn total_count(&self) -> usize {
        read_lock(&self.tools, "tools").len() + read_lock(&self.deferred, "deferred").len()
    }

    pub fn inventory(&self) -> ToolInventory {
        let tools = read_lock(&self.tools, "tools");
        let deferred = read_lock(&self.deferred, "deferred");
        let duplicates = read_lock(&self.duplicate_registrations, "duplicate_registrations");
        let mut duplicate_tool_names = duplicates.keys().cloned().collect::<Vec<_>>();
        duplicate_tool_names.sort();
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
            tool_search_reason: read_lock(&self.tool_search_reason, "tool_search_reason").clone(),
            duplicate_registration_count: duplicates.len(),
            duplicate_tool_names,
            activation_count: self.activation_count.load(Ordering::Relaxed),
            last_activated_tool: read_lock(&self.last_activated_tool, "last_activated_tool")
                .clone(),
        }
    }

    pub fn duplicate_registrations(&self) -> Vec<DuplicateToolRegistration> {
        let mut items = self
            .duplicate_registrations
            .read()
            .unwrap_or_else(|poisoned| {
                tracing::warn!(
                    lock = "duplicate_registrations",
                    "Recovering poisoned tool registry lock"
                );
                poisoned.into_inner()
            })
            .values()
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.name.cmp(&right.name));
        items
    }

    fn record_duplicate_if_present(&self, name: &str, incoming_phase: ToolPoolPhase) -> bool {
        let original_phase = if read_lock(&self.tools, "tools").contains_key(name) {
            Some(ToolPoolPhase::Active)
        } else if read_lock(&self.deferred, "deferred").contains_key(name) {
            Some(ToolPoolPhase::Deferred)
        } else {
            None
        };

        let Some(original_phase) = original_phase else {
            return false;
        };

        let mut duplicates = write_lock(&self.duplicate_registrations, "duplicate_registrations");
        let record =
            duplicates
                .entry(name.to_string())
                .or_insert_with(|| DuplicateToolRegistration {
                    name: name.to_string(),
                    original_phase,
                    duplicate_phase: incoming_phase,
                    attempts: 0,
                });
        record.attempts = record.attempts.saturating_add(1);
        record.duplicate_phase = incoming_phase;
        tracing::warn!(
            tool_name = %name,
            original_phase = ?original_phase,
            duplicate_phase = ?incoming_phase,
            attempts = record.attempts,
            "Duplicate tool registration blocked"
        );
        true
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn read_lock<'a, T>(lock: &'a RwLock<T>, name: &'static str) -> RwLockReadGuard<'a, T> {
    lock.read().unwrap_or_else(|poisoned| {
        tracing::warn!(lock = name, "Recovering poisoned tool registry lock");
        poisoned.into_inner()
    })
}

fn write_lock<'a, T>(lock: &'a RwLock<T>, name: &'static str) -> RwLockWriteGuard<'a, T> {
    lock.write().unwrap_or_else(|poisoned| {
        tracing::warn!(lock = name, "Recovering poisoned tool registry lock");
        poisoned.into_inner()
    })
}

#[cfg(test)]
mod tests {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::Arc;

    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::{json, Value};

    use super::*;

    struct DummyTool(&'static str);

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            self.0
        }

        fn description(&self) -> &str {
            "dummy"
        }

        fn parameters_schema(&self) -> Value {
            json!({"type": "object", "properties": {}})
        }

        async fn execute(
            &self,
            _params: Value,
            _ctx: &crate::tool::ToolContext,
        ) -> Result<crate::tool::ToolResult> {
            Ok(crate::tool::ToolResult::success("ok".to_string()))
        }
    }

    struct CustomDefinitionTool;

    #[async_trait]
    impl Tool for CustomDefinitionTool {
        fn name(&self) -> &str {
            "custom_definition"
        }

        fn description(&self) -> &str {
            "default description"
        }

        fn parameters_schema(&self) -> Value {
            json!({"type": "object"})
        }

        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: self.name().to_string(),
                description: "custom description".to_string(),
                parameters: json!({"type": "object", "required": ["value"]}),
            }
        }

        async fn execute(
            &self,
            _params: Value,
            _ctx: &crate::tool::ToolContext,
        ) -> Result<crate::tool::ToolResult> {
            Ok(crate::tool::ToolResult::success("ok".to_string()))
        }
    }

    #[test]
    fn duplicate_registration_is_blocked_and_recorded() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(DummyTool("dup")));
        registry.register(Arc::new(DummyTool("dup")));
        registry.register_deferred(Arc::new(DummyTool("dup")));

        let inventory = registry.inventory();
        assert_eq!(inventory.active_count, 1);
        assert_eq!(inventory.deferred_count, 0);
        assert_eq!(inventory.duplicate_registration_count, 1);
        assert_eq!(inventory.duplicate_tool_names, vec!["dup".to_string()]);
        let records = registry.duplicate_registrations();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].attempts, 2);
    }

    #[test]
    fn definitions_use_tool_definition_method() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(CustomDefinitionTool));

        let definitions = registry.definitions();
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].description, "custom description");
        assert_eq!(
            definitions[0].parameters,
            json!({"type": "object", "required": ["value"]})
        );
    }

    #[test]
    fn registry_recovers_poisoned_locks() {
        let registry = ToolRegistry::new();
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = registry.tools.write().expect("lock tools");
            panic!("poison registry tools lock");
        }));

        registry.register(Arc::new(DummyTool("after_poison")));

        assert!(registry.get("after_poison").is_some());
        assert_eq!(registry.total_count(), 1);
    }
}
