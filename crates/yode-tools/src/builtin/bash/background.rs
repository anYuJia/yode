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
    ) -> Result<ToolResult> {
        crate::builtin::shell_runtime::execute_background_shell(
            crate::builtin::shell_runtime::BackgroundShellSpec {
                executable: Path::new("sh"),
                args: vec!["-c".to_string(), command.to_string()],
                command_display: command,
                task_kind: "bash",
                description_prefix: "Background bash",
                start_message: "Command started in background",
            },
            working_dir,
            ctx,
        )
        .await
    }
}
