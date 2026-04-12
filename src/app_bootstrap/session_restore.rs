use std::path::PathBuf;

use anyhow::Result;
use tracing::{info, warn};

use crate::Cli;
use yode_core::config::Config;
use yode_core::context::AgentContext;
use yode_core::db::{Database, StoredMessage};
use yode_core::permission::PermissionManager;
use yode_core::session::Session;
use yode_llm::types::{ContentBlock, Message, Role, ToolCall};

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionRestoreReport {
    pub mode: &'static str,
    pub fallback_reason: Option<String>,
    pub decoded_messages: usize,
    pub skipped_messages: usize,
}

pub(crate) fn configure_permissions(config: &Config) -> PermissionManager {
    let mut permissions =
        PermissionManager::from_confirmation_list(config.tools.require_confirmation.clone());

    if let Some(mode_str) = &config.permissions.default_mode {
        if let Ok(mode) = mode_str.parse::<yode_core::PermissionMode>() {
            permissions.set_mode(mode);
        }
    }

    use yode_core::permission::{PermissionRule, RuleBehavior, RuleSource};
    for entry in &config.permissions.always_allow {
        permissions.add_rule(PermissionRule {
            source: RuleSource::UserConfig,
            behavior: RuleBehavior::Allow,
            tool_name: entry.tool.clone(),
            pattern: entry.pattern.clone(),
        });
    }
    for entry in &config.permissions.always_deny {
        permissions.add_rule(PermissionRule {
            source: RuleSource::UserConfig,
            behavior: RuleBehavior::Deny,
            tool_name: entry.tool.clone(),
            pattern: entry.pattern.clone(),
        });
    }

    permissions
}

pub(crate) fn restore_or_create_context(
    cli: &Cli,
    db: &Database,
    workdir: PathBuf,
    provider_name: String,
    model: String,
) -> Result<(AgentContext, Option<Vec<Message>>, SessionRestoreReport)> {
    if let Some(resume_id) = &cli.resume {
        if let Some(session) = resume_session_metadata(db, resume_id)? {
            info!("Resuming session: {}", resume_id);
            let context = AgentContext::resume(
                session.id.clone(),
                workdir,
                session.provider.clone(),
                session.model.clone(),
            );
            let (messages, report) = restore_messages_full(db, resume_id)?;
            return Ok((context, Some(messages), report));
        }

        eprintln!("会话 '{}' 未找到，创建新会话。", resume_id);
        return Ok((
            AgentContext::new(workdir, provider_name, model),
            None,
            SessionRestoreReport {
                mode: "new_session",
                fallback_reason: Some("resume_session_not_found".to_string()),
                decoded_messages: 0,
                skipped_messages: 0,
            },
        ));
    }

    Ok((
        AgentContext::new(workdir, provider_name, model),
        None,
        SessionRestoreReport {
            mode: "new_session",
            fallback_reason: None,
            decoded_messages: 0,
            skipped_messages: 0,
        },
    ))
}

pub(crate) fn ensure_session_exists(db: &Database, context: &AgentContext) -> Result<()> {
    if context.is_resumed {
        return Ok(());
    }

    let session = Session {
        id: context.session_id.clone(),
        name: None,
        provider: context.provider.clone(),
        model: context.model.clone(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    db.create_session(&session)?;
    Ok(())
}

pub(crate) async fn shutdown_mcp_clients(clients: Vec<yode_mcp::McpClient>) {
    for client in clients {
        if let Err(err) = client.shutdown().await {
            warn!(error = %err, "Error shutting down MCP client");
        }
    }
}

fn resume_session_metadata(db: &Database, resume_id: &str) -> Result<Option<Session>> {
    db.get_session(resume_id)
}

fn restore_messages_full(
    db: &Database,
    resume_id: &str,
) -> Result<(Vec<Message>, SessionRestoreReport)> {
    let stored = db.load_messages(resume_id)?;
    let total = stored.len();
    let decoded_messages = stored
        .into_iter()
        .filter_map(stored_message_to_message)
        .collect::<Vec<_>>();
    let report = SessionRestoreReport {
        mode: "full_transcript_restore",
        fallback_reason: None,
        decoded_messages: decoded_messages.len(),
        skipped_messages: total.saturating_sub(decoded_messages.len()),
    };
    Ok((decoded_messages, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use yode_core::session::Session;

    fn test_cli(resume: Option<&str>) -> crate::Cli {
        crate::Cli {
            provider: None,
            model: None,
            config: None,
            workdir: None,
            resume: resume.map(str::to_string),
            serve_mcp: false,
            chat_message: None,
            command: None,
        }
    }

    fn test_db() -> Database {
        let dir = std::env::temp_dir().join(format!(
            "yode-session-restore-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Database::open(&dir.join("sessions.db")).unwrap()
    }

    #[test]
    fn restore_path_uses_metadata_then_full_messages() {
        let db = test_db();
        db.create_session(&Session {
            id: "resume-1".to_string(),
            name: None,
            provider: "anthropic".to_string(),
            model: "claude".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
        db.save_message("resume-1", "user", Some("hello"), None, None, None)
            .unwrap();

        let (context, restored, report) = restore_or_create_context(
            &test_cli(Some("resume-1")),
            &db,
            std::env::temp_dir(),
            "openai".to_string(),
            "gpt".to_string(),
        )
        .unwrap();

        assert!(context.is_resumed);
        assert_eq!(report.mode, "full_transcript_restore");
        assert_eq!(report.decoded_messages, 1);
        assert_eq!(restored.unwrap().len(), 1);
    }

    #[test]
    fn restore_path_reports_missing_session_fallback() {
        let db = test_db();
        let (context, restored, report) = restore_or_create_context(
            &test_cli(Some("missing")),
            &db,
            std::env::temp_dir(),
            "openai".to_string(),
            "gpt".to_string(),
        )
        .unwrap();

        assert!(!context.is_resumed);
        assert!(restored.is_none());
        assert_eq!(report.mode, "new_session");
        assert_eq!(
            report.fallback_reason.as_deref(),
            Some("resume_session_not_found")
        );
    }

    #[test]
    fn restore_path_tracks_skipped_message_decodes() {
        let db = test_db();
        db.create_session(&Session {
            id: "resume-2".to_string(),
            name: None,
            provider: "anthropic".to_string(),
            model: "claude".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
        db.save_message("resume-2", "user", Some("ok"), None, None, None)
            .unwrap();
        db.save_message("resume-2", "unknown-role", Some("skip"), None, None, None)
            .unwrap();

        let (_messages, report) = restore_messages_full(&db, "resume-2").unwrap();
        assert_eq!(report.decoded_messages, 1);
        assert_eq!(report.skipped_messages, 1);
    }
}

fn stored_message_to_message(message: StoredMessage) -> Option<Message> {
    let role = match message.role.as_str() {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        "system" => Role::System,
        _ => return None,
    };
    let tool_calls: Vec<ToolCall> = message
        .tool_calls_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    let mut blocks = Vec::new();
    if let Some(reasoning) = &message.reasoning {
        blocks.push(ContentBlock::Thinking {
            thinking: reasoning.clone(),
            signature: None,
        });
    }
    if let Some(content) = &message.content {
        blocks.push(ContentBlock::Text {
            text: content.clone(),
        });
    }

    Some(
        Message {
            role,
            content: message.content,
            content_blocks: blocks,
            reasoning: message.reasoning,
            tool_calls,
            tool_call_id: message.tool_call_id,
            images: Vec::new(),
        }
        .normalized(),
    )
}
