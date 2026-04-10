pub mod agent;
pub mod ask_user;
pub mod bash;
pub mod batch;
pub mod common;
pub mod coordinator;
pub mod cron;
pub mod edit_file;
pub mod file_diff;
pub mod git_commit;
pub mod git_diff;
pub mod git_log;
pub mod git_status;
pub mod glob;
pub mod grep;
pub mod hypothesis;
pub mod ls;
pub mod lsp;
pub mod mcp_resources;
pub mod memory;
pub mod multi_edit;
pub mod notebook_edit;
pub mod plan_mode;
pub mod project_map;
pub mod read_file;
pub mod review_common;
pub mod review_changes;
pub mod review_pipeline;
pub mod review_then_commit;
pub mod skill;
pub mod task_output;
pub mod test_runner;
pub mod verification_agent;
pub mod todo;
pub mod tool_search;
pub mod web_fetch;
pub mod web_search;
pub mod workflow;
pub mod worktree;
pub mod write_file;

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
    registry.register(Arc::new(edit_file::SnipTool));
    registry.register(Arc::new(bash::BashTool));
    registry.register(Arc::new(glob::GlobTool));
    registry.register(Arc::new(grep::GrepTool));
    registry.register(Arc::new(ls::LsTool));
    registry.register(Arc::new(multi_edit::MultiEditTool));
    registry.register(Arc::new(web_fetch::WebFetchTool));
    registry.register(Arc::new(web_fetch::WebBrowserTool));
    registry.register(Arc::new(web_search::WebSearchTool));
    registry.register(Arc::new(todo::TodoTool));
    registry.register(Arc::new(todo::TaskCreateTool));
    registry.register(Arc::new(todo::TaskListTool));
    registry.register(Arc::new(todo::TaskGetTool));
    registry.register(Arc::new(batch::BatchTool));
    registry.register(Arc::new(ask_user::AskUserTool));
    registry.register(Arc::new(memory::MemoryTool));
    registry.register(Arc::new(task_output::TaskOutputTool));
    registry.register(Arc::new(notebook_edit::NotebookEditTool));
    registry.register(Arc::new(worktree::EnterWorktreeTool));
    registry.register(Arc::new(worktree::ExitWorktreeTool));
    registry.register(Arc::new(mcp_resources::ListMcpResourcesTool));
    registry.register(Arc::new(mcp_resources::ReadMcpResourceTool));
    registry.register(Arc::new(mcp_resources::McpAuthTool));
    registry.register(Arc::new(tool_search::ToolSearchTool));
    registry.register(Arc::new(cron::CronCreateTool));
    registry.register(Arc::new(cron::CronListTool));
    registry.register(Arc::new(cron::CronDeleteTool));
    registry.register(Arc::new(lsp::LspTool));
    registry.register(Arc::new(agent::AgentTool));
    registry.register(Arc::new(plan_mode::EnterPlanModeTool));
    registry.register(Arc::new(plan_mode::ExitPlanModeTool));
    registry.register(Arc::new(plan_mode::VerifyPlanExecutionTool));
    registry.register(Arc::new(verification_agent::VerificationAgentTool));
    registry.register(Arc::new(review_changes::ReviewChangesTool));
    registry.register(Arc::new(review_pipeline::ReviewPipelineTool));
    registry.register(Arc::new(review_then_commit::ReviewThenCommitTool));
    registry.register(Arc::new(project_map::ProjectMapTool));
    registry.register(Arc::new(hypothesis::HypothesisTool));
    registry.register(Arc::new(file_diff::FileDiffTool));
    registry.register(Arc::new(git_commit::GitCommitTool));
    registry.register(Arc::new(git_diff::GitDiffTool));
    registry.register(Arc::new(git_log::GitLogTool));
    registry.register(Arc::new(git_status::GitStatusTool));
    registry.register(Arc::new(common::SendUserMessageTool));
    registry.register(Arc::new(common::ConfigTool));
    registry.register(Arc::new(common::SleepTool));
    registry.register(Arc::new(common::SendUserFileTool));
    registry.register(Arc::new(common::REPLTool));
    registry.register(Arc::new(coordinator::CoordinateAgentsTool));
    registry.register(Arc::new(workflow::WorkflowRunTool));
    registry.register(Arc::new(workflow::WorkflowRunWithWritesTool));
}

/// Register the skill tool with the given skill store.
pub fn register_skill_tool(registry: &mut ToolRegistry, store: Arc<Mutex<skill::SkillStore>>) {
    registry.register(Arc::new(skill::SkillTool {
        store: store.clone(),
    }));
    registry.register(Arc::new(skill::discover::DiscoverSkillsTool { store }));
}
