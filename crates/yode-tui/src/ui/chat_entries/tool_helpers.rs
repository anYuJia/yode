use serde_json::Value;

pub(super) fn truncate_ellipsis(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    format!("{}...", text.chars().take(max_chars).collect::<String>())
}

pub(super) fn tool_summary_value(name: &str, args: &Value) -> String {
    match name {
        "bash" => args["command"].as_str().unwrap_or("???").to_string(),
        "write_file" | "read_file" => args["file_path"].as_str().unwrap_or("???").to_string(),
        "edit_file" => args["file_path"].as_str().unwrap_or("???").to_string(),
        "glob" => args["pattern"].as_str().unwrap_or("???").to_string(),
        "grep" => args["pattern"].as_str().unwrap_or("???").to_string(),
        _ => args
            .as_object()
            .and_then(|object| {
                ["command", "path", "file_path", "query", "pattern", "url"]
                    .iter()
                    .find_map(|key| object.get(*key).and_then(|value| value.as_str()))
            })
            .unwrap_or("")
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{tool_summary_value, truncate_ellipsis};

    #[test]
    fn truncate_ellipsis_shortens_long_text() {
        assert_eq!(truncate_ellipsis("abcdef", 4), "abcd...");
    }

    #[test]
    fn tool_summary_value_prefers_common_keys() {
        assert_eq!(
            tool_summary_value("bash", &json!({"command": "cargo test"})),
            "cargo test"
        );
        assert_eq!(
            tool_summary_value("unknown", &json!({"url": "https://example.com"})),
            "https://example.com"
        );
    }
}
