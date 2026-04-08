use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::tool::Tool;

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
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
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
