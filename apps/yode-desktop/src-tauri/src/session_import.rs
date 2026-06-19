use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use uuid::Uuid;
use yode_core::db::Database;
use yode_core::session::Session;

pub(super) async fn collect_import_files(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = paths;
    while let Some(path) = stack.pop() {
        let Ok(metadata) = tokio::fs::metadata(&path).await else {
            continue;
        };
        if metadata.is_dir() {
            if let Ok(mut entries) = tokio::fs::read_dir(&path).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    stack.push(entry.path());
                }
            }
            continue;
        }
        let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        if matches!(
            ext.to_lowercase().as_str(),
            "json" | "jsonl" | "md" | "markdown" | "txt"
        ) {
            files.push(path);
        }
    }
    files
}

pub(super) async fn import_one_ai_session(
    db: &Database,
    path: &Path,
    provider: &str,
    model: &str,
) -> Result<Option<Session>> {
    let text = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("无法读取导入文件 {}", path.display()))?;
    let messages = parse_import_messages(&text, path);
    if messages.is_empty() {
        return Ok(None);
    }

    let now = Utc::now();
    let title = import_title(path, &messages);
    let session = Session {
        id: Uuid::new_v4().to_string(),
        name: Some(title),
        project_root: None,
        provider: provider.to_string(),
        model: model.to_string(),
        created_at: now,
        updated_at: now,
    };
    db.create_session(&session)?;
    for (role, content) in messages {
        db.save_message(&session.id, &role, Some(&content), None, None, None)?;
    }
    db.touch_session(&session.id)?;
    Ok(Some(session))
}

fn parse_import_messages(text: &str, path: &Path) -> Vec<(String, String)> {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext == "jsonl" {
        let mut messages = Vec::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                append_messages_from_json(&value, &mut messages);
            }
        }
        return messages;
    }
    if ext == "json" {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
            let mut messages = Vec::new();
            append_messages_from_json(&value, &mut messages);
            if !messages.is_empty() {
                return messages;
            }
        }
    }
    vec![("user".to_string(), text.trim().to_string())]
        .into_iter()
        .filter(|(_, content)| !content.is_empty())
        .collect()
}

fn append_messages_from_json(value: &serde_json::Value, out: &mut Vec<(String, String)>) {
    if let Some(array) = value.as_array() {
        for item in array {
            append_messages_from_json(item, out);
        }
        return;
    }
    if let Some(messages) = value.get("messages").and_then(|value| value.as_array()) {
        for message in messages {
            append_messages_from_json(message, out);
        }
        return;
    }
    if let Some(mapping) = value.as_object() {
        let role = mapping
            .get("role")
            .or_else(|| mapping.get("author"))
            .or_else(|| mapping.get("sender"))
            .and_then(|value| value.as_str())
            .unwrap_or("user")
            .to_lowercase();
        let normalized_role = if role.contains("assistant") || role.contains("bot") {
            "assistant"
        } else if role.contains("system") {
            "system"
        } else {
            "user"
        };
        let content = mapping
            .get("content")
            .or_else(|| mapping.get("text"))
            .or_else(|| mapping.get("message"))
            .and_then(extract_json_text)
            .unwrap_or_default();
        if !content.trim().is_empty() {
            out.push((normalized_role.to_string(), content));
        }
    }
}

fn extract_json_text(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    if let Some(array) = value.as_array() {
        let parts: Vec<String> = array.iter().filter_map(extract_json_text).collect();
        return Some(parts.join("\n"));
    }
    if let Some(object) = value.as_object() {
        if let Some(text) = object.get("text").and_then(|value| value.as_str()) {
            return Some(text.to_string());
        }
        if let Some(parts) = object.get("parts").and_then(|value| value.as_array()) {
            let parts: Vec<String> = parts.iter().filter_map(extract_json_text).collect();
            return Some(parts.join("\n"));
        }
    }
    None
}

fn import_title(path: &Path, messages: &[(String, String)]) -> String {
    let fallback = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("导入会话");
    let first = messages
        .iter()
        .find(|(role, _)| role == "user")
        .map(|(_, content)| content.trim())
        .filter(|content| !content.is_empty())
        .unwrap_or(fallback);
    let mut title = first.chars().take(36).collect::<String>();
    if first.chars().count() > 36 {
        title.push('…');
    }
    format!("导入：{}", title)
}
