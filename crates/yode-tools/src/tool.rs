use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};

use crate::registry::ToolRegistry;
use crate::state::TaskStore;

/// A query sent to the user via the TUI.
#[derive(Debug, Clone)]
pub struct UserQuery {
    pub id: String,
    pub question: String,
}

/// Sub-agent runner trait (implemented by yode-core).
pub trait SubAgentRunner: Send + Sync {
    fn run_sub_agent(
        &self,
        prompt: String,
        allowed_tools: Vec<String>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>>;
}

/// MCP resource provider trait (implemented by yode-core/yode-mcp).
pub trait McpResourceProvider: Send + Sync {
    fn list_resources(
        &self,
        server: Option<&str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<McpResource>>> + Send + '_>>;

    fn read_resource(
        &self,
        server: &str,
        uri: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>>;
}

/// MCP resource descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub server: String,
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
}

/// Worktree state for enter/exit worktree tools.
#[derive(Debug, Default)]
pub struct WorktreeState {
    pub original_dir: Option<PathBuf>,
    pub current_worktree: Option<PathBuf>,
    pub branch_name: Option<String>,
}

/// Progress update from a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProgress {
    /// Message describing current activity (e.g. "Building 3/10...").
    pub message: String,
    /// Completion percentage (0-100), if known.
    pub percent: Option<u8>,
}

/// Context passed to every tool execution, providing access to shared resources.
pub struct ToolContext {
    /// Access to the full tool registry (needed by `batch`).
    pub registry: Option<Arc<ToolRegistry>>,
    /// Shared task store (needed by `todo`).
    pub tasks: Option<Arc<Mutex<TaskStore>>>,
    /// Channel to send questions to the user (needed by `ask_user`).
    pub user_input_tx: Option<mpsc::UnboundedSender<UserQuery>>,
    /// Channel to receive answers from the user (needed by `ask_user`).
    pub user_input_rx: Option<Arc<Mutex<mpsc::UnboundedReceiver<String>>>>,
    /// Channel to send progress updates back to the engine.
    pub progress_tx: Option<mpsc::UnboundedSender<ToolProgress>>,
    /// Current working directory.
    pub working_dir: Option<PathBuf>,
    /// Sub-agent runner for the `agent` tool.
    pub sub_agent_runner: Option<Arc<dyn SubAgentRunner>>,
    /// MCP resource provider for list/read MCP resources.
    pub mcp_resources: Option<Arc<dyn McpResourceProvider>>,
    /// Cron job manager.
    pub cron_manager: Option<Arc<Mutex<crate::cron_manager::CronManager>>>,
    /// LSP manager.
    pub lsp_manager: Option<Arc<Mutex<crate::lsp_manager::LspManager>>>,
    /// Git worktree state.
    pub worktree_state: Option<Arc<Mutex<WorktreeState>>>,
    /// Whether engine is in plan mode (read-only tools only).
    pub plan_mode: Option<Arc<Mutex<bool>>>,
}

impl ToolContext {
    /// Create an empty context (no shared resources).
    pub fn empty() -> Self {
        Self {
            registry: None,
            tasks: None,
            user_input_tx: None,
            user_input_rx: None,
            progress_tx: None,
            working_dir: None,
            sub_agent_runner: None,
            mcp_resources: None,
            cron_manager: None,
            lsp_manager: None,
            worktree_state: None,
            plan_mode: None,
        }
    }
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
    pub error_type: Option<ToolErrorType>,
    pub recoverable: bool,
    pub suggestion: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ToolErrorType {
    Validation,
    Protocol,
    NotFound,
    PermissionDeny,
    Permission,
    Execution,
    QuotaExceeded,
    Timeout,
    Unknown,
}

impl ToolResult {
    pub fn success(content: String) -> Self {
        Self {
            content,
            is_error: false,
            error_type: None,
            recoverable: false,
            suggestion: None,
            metadata: None,
        }
    }

    pub fn success_with_metadata(content: String, metadata: Value) -> Self {
        Self {
            content,
            is_error: false,
            error_type: None,
            recoverable: false,
            suggestion: None,
            metadata: Some(metadata),
        }
    }

    pub fn error(content: String) -> Self {
        Self {
            content,
            is_error: true,
            error_type: Some(ToolErrorType::Execution),
            recoverable: false,
            suggestion: None,
            metadata: None,
        }
    }

    pub fn error_typed(
        content: String,
        error_type: ToolErrorType,
        recoverable: bool,
        suggestion: Option<String>,
    ) -> Self {
        Self {
            content,
            is_error: true,
            error_type: Some(error_type),
            recoverable,
            suggestion,
            metadata: None,
        }
    }
}

/// Tool definition for LLM (serializable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema
}

/// Tool capability flags
#[derive(Debug, Clone, Copy, Default)]
pub struct ToolCapabilities {
    /// Requires user confirmation before execution
    pub requires_confirmation: bool,
    /// Can be executed without user interaction
    pub supports_auto_execution: bool,
    /// Is a read-only operation (safe)
    pub read_only: bool,
}

/// Tool trait - implemented by builtin tools, MCP tools, etc.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    
    /// User-facing name for the tool (e.g. "Bash" for "bash").
    fn user_facing_name(&self) -> &str {
        self.name()
    }

    /// Short description of what the tool is doing with the given params.
    /// Used for progress display (e.g. "Reading Cargo.toml").
    fn activity_description(&self, _params: &Value) -> String {
        format!("Executing {}", self.name())
    }

    /// Get tool capabilities
    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities::default()
    }
    
    /// Legacy method - check if requires confirmation
    fn requires_confirmation(&self) -> bool {
        self.capabilities().requires_confirmation
    }
    
    /// Execute the tool with given parameters
    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult>;
    
    /// Get tool definition for LLM
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }
}
