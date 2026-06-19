use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::json;

use super::DesktopRuntime;
use crate::desktop_settings_store::{
    desktop_bool_setting, desktop_string_setting, read_desktop_settings,
    write_desktop_settings_async,
};
use crate::protocol::{DesktopActionResult, PersonalizationState};

impl DesktopRuntime {
    pub fn personalization_state(&self) -> Result<PersonalizationState> {
        personalization_state_from_settings(&read_desktop_settings()?)
    }

    pub async fn personalization_reset_memories(&self) -> Result<DesktopActionResult> {
        let mut removed = 0usize;
        for root in self.memory_roots()? {
            for path in [
                yode_core::session_memory::session_memory_path(&root),
                yode_core::session_memory::live_session_memory_path(&root),
                root.join("MEMORY.md"),
            ] {
                if tokio::fs::try_exists(&path).await? {
                    tokio::fs::remove_file(&path).await.with_context(|| {
                        format!("Failed to remove memory file: {}", path.display())
                    })?;
                    removed += 1;
                }
            }
            let memory_dir = root.join(".yode").join("memory");
            if tokio::fs::try_exists(&memory_dir).await? {
                tokio::fs::remove_dir_all(&memory_dir)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to remove memory directory: {}",
                            memory_dir.display()
                        )
                    })?;
                removed += 1;
            }
        }

        let mut settings = read_desktop_settings()?;
        settings.insert("yode-enable-memories".to_string(), json!(false));
        settings.insert("yode-skip-tool-chats".to_string(), json!(false));
        write_desktop_settings_async(&settings).await?;

        Ok(DesktopActionResult {
            ok: true,
            message: if removed == 0 {
                "未发现需要清理的长期记忆，已关闭长期记忆。".to_string()
            } else {
                format!("已清理 {} 个长期记忆文件或目录，并关闭长期记忆。", removed)
            },
            path: None,
        })
    }

    fn memory_roots(&self) -> Result<Vec<PathBuf>> {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();
        for root in [
            self.workspace_path.clone(),
            dirs::home_dir().unwrap_or_else(|| self.workspace_path.clone()),
        ] {
            let key = root.display().to_string();
            if seen.insert(key) {
                roots.push(root);
            }
        }
        for session in self.db.list_sessions(1_000)? {
            if let Some(root) = session
                .project_root
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
            {
                let key = root.display().to_string();
                if seen.insert(key) {
                    roots.push(root);
                }
            }
        }
        Ok(roots)
    }
}

fn personalization_state_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> Result<PersonalizationState> {
    Ok(PersonalizationState {
        personality: desktop_string_setting(settings, "yode-personality", "Friendly"),
        custom_instructions: desktop_string_setting(settings, "yode-custom-instructions", ""),
        enable_memories: desktop_bool_setting(settings, "yode-enable-memories", false),
        skip_tool_chats: desktop_bool_setting(settings, "yode-skip-tool-chats", false),
    })
}

pub(super) fn build_personalization_prompt(state: &PersonalizationState) -> Option<String> {
    let mut lines = Vec::new();
    match state.personality.as_str() {
        "Professional" => lines.push(
            "Tone: professional, rigorous, precise, and calm. Prefer concrete tradeoffs and clear verification notes.",
        ),
        "Concise" => lines.push(
            "Tone: concise and direct. Keep explanations compact while still naming important risks and verification.",
        ),
        _ => lines.push(
            "Tone: friendly, warm, and collaborative. Stay clear and practical without becoming verbose.",
        ),
    }

    let custom = state.custom_instructions.trim();
    if !custom.is_empty() {
        lines.push("Host-level custom instructions from the user:");
        lines.push(custom);
    }

    Some(lines.join("\n"))
}
