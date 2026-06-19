use anyhow::Result;

use super::DesktopRuntime;
use crate::protocol::ImportAiSessionsResult;
use crate::session_import::{collect_import_files, import_one_ai_session};

impl DesktopRuntime {
    pub async fn import_ai_sessions(&self) -> Result<ImportAiSessionsResult> {
        let Some(paths) = rfd::FileDialog::new()
            .set_title("选择要导入的 AI 会话文件或目录")
            .add_filter("会话文件", &["json", "jsonl", "md", "markdown", "txt"])
            .pick_files()
        else {
            return Ok(ImportAiSessionsResult {
                imported: 0,
                skipped: 0,
                sessions: Vec::new(),
            });
        };

        let (provider, model) = {
            let config = self
                .config
                .lock()
                .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
            self.default_llm_for_new_session(&config)?
        };

        let mut imported_sessions = Vec::new();
        let mut skipped = 0usize;
        for file in collect_import_files(paths).await {
            match import_one_ai_session(&self.db, &file, &provider, &model).await {
                Ok(Some(session)) => imported_sessions.push(self.map_session(session, None)),
                Ok(None) => skipped += 1,
                Err(err) => {
                    tracing::warn!("Failed to import {}: {}", file.display(), err);
                    skipped += 1;
                }
            }
        }

        Ok(ImportAiSessionsResult {
            imported: imported_sessions.len(),
            skipped,
            sessions: imported_sessions,
        })
    }
}
