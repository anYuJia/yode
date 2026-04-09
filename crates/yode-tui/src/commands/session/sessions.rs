use std::path::PathBuf;

use yode_core::db::Database;

use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct SessionsCommand {
    meta: CommandMeta,
}

impl SessionsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "sessions",
                description: "List recent sessions",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for SessionsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let db_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".yode")
            .join("sessions.db");
        let transcripts_dir = PathBuf::from(&ctx.session.working_dir)
            .join(".yode")
            .join("transcripts");
        match Database::open(&db_path) {
            Ok(db) => match db.list_sessions(10) {
                Ok(sessions) if sessions.is_empty() => Ok(CommandOutput::Message(
                    "No saved sessions found.".to_string(),
                )),
                Ok(sessions) => {
                    let mut lines = String::from("Recent sessions:\n");
                    for s in &sessions {
                        let id_short = &s.id[..s.id.len().min(8)];
                        let age = chrono::Utc::now().signed_duration_since(s.updated_at);
                        let age_str = if age.num_days() > 0 {
                            format!("{}d ago", age.num_days())
                        } else if age.num_hours() > 0 {
                            format!("{}h ago", age.num_hours())
                        } else {
                            format!("{}m ago", age.num_minutes().max(1))
                        };
                        let name = s.name.as_deref().unwrap_or("-");
                        lines.push_str(&format!(
                            "  {}  {:<12} {:<8} {}\n",
                            id_short, s.model, age_str, name
                        ));
                        if let Some(preview) =
                            latest_transcript_preview_for_session(&transcripts_dir, &s.id)
                        {
                            lines.push_str(&format!(
                                "      transcript: {} · {} · {}\n",
                                preview.mode.unwrap_or_else(|| "unknown".to_string()),
                                preview
                                    .timestamp
                                    .unwrap_or_else(|| "unknown time".to_string()),
                                preview
                                    .summary_preview
                                    .unwrap_or_else(|| "no summary".to_string())
                            ));
                        }
                    }
                    lines.push_str("\nResume with: yode --resume <session-id>");
                    Ok(CommandOutput::Message(lines))
                }
                Err(e) => Err(format!("Failed to list sessions: {}", e)),
            },
            Err(e) => Err(format!("Failed to open session database: {}", e)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionTranscriptPreview {
    mode: Option<String>,
    timestamp: Option<String>,
    summary_preview: Option<String>,
}

fn latest_transcript_preview_for_session(
    transcripts_dir: &std::path::Path,
    session_id: &str,
) -> Option<SessionTranscriptPreview> {
    let short_id = session_id.chars().take(8).collect::<String>();
    let mut entries = std::fs::read_dir(transcripts_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("md")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with(&format!("{}-compact-", short_id)))
                    .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    let path = entries.into_iter().next()?;
    let content = std::fs::read_to_string(path).ok()?;
    let mut mode = None;
    let mut timestamp = None;
    for line in content.lines().take(10) {
        if let Some(value) = line.strip_prefix("- Mode: ") {
            mode = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("- Timestamp: ") {
            timestamp = Some(value.to_string());
        }
    }

    Some(SessionTranscriptPreview {
        mode,
        timestamp,
        summary_preview: extract_summary_preview(&content),
    })
}

fn extract_summary_preview(content: &str) -> Option<String> {
    let start = content.find("## Summary Anchor")?;
    let summary_block = &content[start..];
    let fenced_start = summary_block.find("```text")?;
    let after_fence = &summary_block[fenced_start + "```text".len()..];
    let fenced_end = after_fence.find("```")?;
    let summary = after_fence[..fenced_end].trim();
    if summary.is_empty() {
        return None;
    }

    let preview: String = summary.chars().take(120).collect();
    if summary.chars().count() > 120 {
        Some(format!("{}...", preview))
    } else {
        Some(preview)
    }
}

#[cfg(test)]
mod tests {
    use super::latest_transcript_preview_for_session;

    #[test]
    fn latest_transcript_preview_uses_matching_session_prefix() {
        let dir = std::env::temp_dir().join(format!(
            "yode-sessions-transcript-preview-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("abc12345-compact-20250101.md"),
            "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n\n## Summary Anchor\n\n```text\nolder\n```\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("abc12345-compact-20260101.md"),
            "# Compaction Transcript\n\n- Mode: manual\n- Timestamp: 2026-01-02 10:00:00\n\n## Summary Anchor\n\n```text\nnewer summary\n```\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("zzz99999-compact-20270101.md"),
            "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-03 10:00:00\n",
        )
        .unwrap();

        let preview = latest_transcript_preview_for_session(&dir, "abc12345-long-session").unwrap();
        assert_eq!(preview.mode.as_deref(), Some("manual"));
        assert_eq!(preview.timestamp.as_deref(), Some("2026-01-02 10:00:00"));
        assert_eq!(preview.summary_preview.as_deref(), Some("newer summary"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
