pub mod agent;
pub mod ask_user;
pub mod bash;
pub mod batch;
pub mod cron;
pub mod edit_file;
pub mod glob;
pub mod grep;
pub mod lsp;
pub mod ls;
pub mod mcp_resources;
pub mod memory;
pub mod multi_edit;
pub mod notebook_edit;
pub mod plan_mode;
pub mod read_file;
pub mod skill;
pub mod todo;
pub mod tool_search;
pub mod web_fetch;
pub mod web_search;
pub mod worktree;
pub mod write_file;

use std::sync::Arc;

use tokio::sync::Mutex;

use crate::registry::ToolRegistry;

pub fn register_builtin_tools(registry: &mut ToolRegistry) {
    registry.register(Arc::new(read_file::ReadFileTool));
    registry.register(Arc::new(write_file::WriteFileTool));
    registry.register(Arc::new(edit_file::EditFileTool));
    registry.register(Arc::new(bash::BashTool));
    registry.register(Arc::new(glob::GlobTool));
    registry.register(Arc::new(grep::GrepTool));
    registry.register(Arc::new(ls::LsTool));
    registry.register(Arc::new(multi_edit::MultiEditTool));
    registry.register(Arc::new(web_fetch::WebFetchTool));
    registry.register(Arc::new(web_search::WebSearchTool));
    registry.register(Arc::new(todo::TodoTool));
    registry.register(Arc::new(batch::BatchTool));
    registry.register(Arc::new(ask_user::AskUserTool));
    registry.register(Arc::new(memory::MemoryTool));
    registry.register(Arc::new(notebook_edit::NotebookEditTool));
    registry.register(Arc::new(worktree::EnterWorktreeTool));
    registry.register(Arc::new(worktree::ExitWorktreeTool));
    registry.register(Arc::new(mcp_resources::ListMcpResourcesTool));
    registry.register(Arc::new(mcp_resources::ReadMcpResourceTool));
    registry.register(Arc::new(tool_search::ToolSearchTool));
    registry.register(Arc::new(cron::CronTool));
    registry.register(Arc::new(lsp::LspTool));
    registry.register(Arc::new(agent::AgentTool));
    registry.register(Arc::new(plan_mode::EnterPlanModeTool));
    registry.register(Arc::new(plan_mode::ExitPlanModeTool));
}

/// Register the skill tool with the given skill store.
pub fn register_skill_tool(registry: &mut ToolRegistry, store: Arc<Mutex<skill::SkillStore>>) {
    registry.register(Arc::new(skill::SkillTool { store }));
}
