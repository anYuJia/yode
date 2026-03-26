pub mod agent;
pub mod ask_user;
pub mod bash;
pub mod batch;
pub mod cron;
pub mod edit_file;
pub mod file_diff;
pub mod git_commit;
mod git_diff;
mod git_log;
mod git_status;
mod glob;
mod grep;
mod hypothesis;
mod ls;
pub mod lsp;
pub mod mcp_resources;
pub mod memory;
pub mod multi_edit;
pub mod notebook_edit;
pub mod plan_mode;
mod project_map;
mod read_file;
pub mod skill;
mod test_runner;
pub mod todo;
pub mod tool_search;
pub mod web_fetch;
pub mod web_search;
pub mod worktree;
mod write_file;

pub use bash::BashTool;
pub use edit_file::EditFileTool;
pub use file_diff::FileDiffTool;
pub use git_commit::GitCommitTool;
pub use git_diff::GitDiffTool;
pub use git_log::GitLogTool;
pub use git_status::GitStatusTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use hypothesis::HypothesisTool;
pub use ls::LsTool;
pub use project_map::ProjectMapTool;
pub use read_file::ReadFileTool;
pub use test_runner::TestRunnerTool;
pub use write_file::WriteFileTool;

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
