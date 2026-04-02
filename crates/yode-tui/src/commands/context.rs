use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use yode_core::engine::AgentEngine;
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;

use crate::app::{ChatEntry, SessionState};
use crate::terminal_caps::TerminalCaps;

pub struct CommandContext<'a> {
    pub engine: Arc<Mutex<AgentEngine>>,
    pub provider_registry: &'a Arc<ProviderRegistry>,
    pub provider_name: &'a mut String,
    pub provider_models: &'a mut Vec<String>,
    pub all_provider_models: &'a HashMap<String, Vec<String>>,
    pub chat_entries: &'a mut Vec<ChatEntry>,
    pub tools: &'a Arc<ToolRegistry>,
    pub session: &'a SessionState,
    pub terminal_caps: &'a TerminalCaps,
    pub input_history: &'a [String],
    pub should_quit: &'a mut bool,
}

pub struct CompletionContext<'a> {
    pub provider_models: &'a [String],
    pub all_provider_models: &'a HashMap<String, Vec<String>>,
    pub provider_name: &'a str,
    pub tools: &'a Arc<ToolRegistry>,
}
