use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

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
    pub printed_count: &'a mut usize,
    pub streaming_buf: &'a mut String,
    pub streaming_printed_lines: &'a mut usize,
    pub streaming_in_code_block: &'a mut bool,
    pub tools: &'a Arc<ToolRegistry>,
    pub session: &'a mut SessionState,
    pub terminal_caps: &'a TerminalCaps,
    pub input_history: &'a [String],
    pub should_quit: &'a mut bool,
    pub session_start: Instant,
    pub turn_started_at: Option<Instant>,
    pub cmd_registry: &'a super::registry::CommandRegistry,
}

pub struct CompletionContext<'a> {
    pub provider_models: &'a [String],
    pub all_provider_models: &'a HashMap<String, Vec<String>>,
    pub provider_name: &'a str,
    pub tools: &'a Arc<ToolRegistry>,
}
