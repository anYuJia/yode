use std::path::Path;

use anyhow::Result;

use super::BashTool;
use crate::tool::{ToolContext, ToolResult};

impl BashTool {
    pub(super) async fn execute_background(
        &self,
        command: &str,
        working_dir: &Path,
        ctx: &ToolContext,
        dangerously_disable_sandbox: bool,
    ) -> Result<ToolResult> {
        let prepared = crate::sandbox::prepare_shell(
            command,
            working_dir,
            dangerously_disable_sandbox,
        )?;
        let sandbox_info = prepared.info.clone();
        let mut result = crate::builtin::shell_runtime::execute_background_shell(
            crate::builtin::shell_runtime::BackgroundShellSpec {
                executable: &prepared.executable,
                args: prepared.args,
                command_display: command,
                task_kind: "bash",
                description_prefix: "Background bash",
                start_message: "Command started in background",
            },
            working_dir,
            ctx,
        )
        .await?;
        crate::sandbox::annotate_tool_result(&mut result, &sandbox_info);
        Ok(result)
    }
}