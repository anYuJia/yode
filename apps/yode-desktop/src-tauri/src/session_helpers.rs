use std::collections::HashMap;

use chrono::Utc;
use yode_core::db::StoredMessage;
use yode_core::session::Session;

pub(super) fn title_from_content(content: &str) -> String {
    let title = content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(28)
        .collect::<String>();

    if title.is_empty() {
        "新对话".to_string()
    } else {
        title
    }
}

pub(super) fn title_from_content_or_images(content: &str, image_count: usize) -> String {
    if !content.trim().is_empty() {
        return title_from_content(content);
    }
    if image_count > 1 {
        format!("{} 张图片", image_count)
    } else {
        "图片".to_string()
    }
}

pub(super) fn render_session_markdown(session: &Session, messages: &[StoredMessage]) -> String {
    let title = session
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Yode 会话导出");
    let mut output = String::new();
    output.push_str("# ");
    output.push_str(title.trim());
    output.push_str("\n\n");
    output.push_str(&format!("- Session: `{}`\n", session.id));
    output.push_str(&format!("- Provider: `{}`\n", session.provider));
    output.push_str(&format!("- Model: `{}`\n", session.model));
    if let Some(project_root) = session.project_root.as_deref() {
        output.push_str(&format!("- Project: `{}`\n", project_root));
    }
    output.push_str(&format!("- Exported at: `{}`\n\n", Utc::now().to_rfc3339()));

    for message in messages {
        output.push_str("## ");
        output.push_str(role_heading(&message.role));
        output.push_str("\n\n");
        if let Some(reasoning) = message
            .reasoning
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            output.push_str("<details><summary>Reasoning</summary>\n\n");
            output.push_str(reasoning.trim());
            output.push_str("\n\n</details>\n\n");
        }
        match message
            .content
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            Some(content) => {
                output.push_str(content.trim());
                output.push_str("\n\n");
            }
            None => output.push_str("_无文本内容_\n\n"),
        }
        if let Some(tool_calls) = message
            .tool_calls_json
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            output.push_str("```json\n");
            output.push_str(tool_calls);
            output.push_str("\n```\n\n");
        }
    }
    output
}

pub(super) fn short_session_id(session_id: &str) -> String {
    session_id.chars().take(8).collect::<String>()
}

pub(super) fn build_local_compaction_summary(
    session: &Session,
    messages: &[StoredMessage],
) -> String {
    let mut role_counts: HashMap<&str, usize> = HashMap::new();
    for message in messages {
        *role_counts.entry(message.role.as_str()).or_default() += 1;
    }
    let first_user = messages
        .iter()
        .find(|message| message.role == "user")
        .and_then(|message| message.content.as_deref())
        .map(compact_summary_line)
        .unwrap_or_else(|| "未找到早期用户消息。".to_string());
    let last_assistant = messages
        .iter()
        .rev()
        .find(|message| message.role == "assistant")
        .and_then(|message| message.content.as_deref())
        .map(compact_summary_line)
        .unwrap_or_else(|| "未找到早期助手回复。".to_string());

    format!(
        "[Context summary] 桌面端已本地压缩较早的会话历史。\n- Session: {}\n- Provider/model: {}/{}\n- Removed messages: {}\n- Role counts: user={}, assistant={}, tool={}, system={}\n- Earliest user intent: {}\n- Latest removed assistant note: {}\n- Note: 这是确定性的本地摘要，用于减少恢复上下文；如需完整原文，请先使用 /export。",
        session.id,
        session.provider,
        session.model,
        messages.len(),
        role_counts.get("user").copied().unwrap_or(0),
        role_counts.get("assistant").copied().unwrap_or(0),
        role_counts.get("tool").copied().unwrap_or(0),
        role_counts.get("system").copied().unwrap_or(0),
        first_user,
        last_assistant,
    )
}

fn compact_summary_line(content: &str) -> String {
    content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(220)
        .collect()
}

fn role_heading(role: &str) -> &'static str {
    match role {
        "user" => "User",
        "assistant" => "Assistant",
        "tool" => "Tool",
        "system" => "System",
        _ => "Message",
    }
}

pub(super) fn stored_message_to_message(
    message: StoredMessage,
) -> Option<yode_llm::types::Message> {
    let role = match message.role.as_str() {
        "user" => yode_llm::types::Role::User,
        "assistant" => yode_llm::types::Role::Assistant,
        "tool" => yode_llm::types::Role::Tool,
        "system" => yode_llm::types::Role::System,
        _ => return None,
    };
    let tool_calls: Vec<yode_llm::types::ToolCall> = message
        .tool_calls_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    let mut blocks = Vec::new();
    if let Some(reasoning) = &message.reasoning {
        blocks.push(yode_llm::types::ContentBlock::Thinking {
            thinking: reasoning.clone(),
            signature: None,
        });
    }
    if let Some(content) = &message.content {
        blocks.push(yode_llm::types::ContentBlock::Text {
            text: content.clone(),
        });
    }

    let images = stored_images(&message);

    Some(
        yode_llm::types::Message {
            role,
            content: message.content,
            content_blocks: blocks,
            reasoning: message.reasoning,
            tool_calls,
            tool_call_id: message.tool_call_id,
            images,
        }
        .normalized(),
    )
}

pub(super) fn stored_images(message: &StoredMessage) -> Vec<yode_llm::types::ImageData> {
    message
        .images_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}
