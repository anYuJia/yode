use super::*;

impl AgentEngine {
    /// Cleans hallucinated protocol tags from LLM text response.
    pub(in crate::engine) fn clean_assistant_response(&self, text: &str) -> String {
        let re = Regex::new(r"(?s)\[DUMMY_TOOL_RESULT\]|\[tool_use\s+[^\]>]+[\]>](?:\s*[:]\s*)?\{.*?\}[\s\]>]*|\[tool_result\s+[^\]>]+[\]>](?:\s*[:]\s*)?\{.*?\}[\s\]>]*|\[tool_(?:use|result)\s+[^\]>]+[\]>]").unwrap();
        re.replace_all(text, "").to_string()
    }

    /// Detects if the assistant response contains forbidden internal protocol patterns.
    pub(in crate::engine) fn is_protocol_violation(&self, text: &str) -> bool {
        let forbidden_patterns = [
            "[tool_use",
            "[DUMMY_TOOL",
            "[tool_result",
            "<tool_code>",
            "<tool_input>",
            "<tool_call>",
        ];

        for pattern in forbidden_patterns {
            if text.contains(pattern) {
                return true;
            }
        }
        false
    }

    /// Attempts to recover tool calls leaked into the text response.
    pub(in crate::engine) fn recover_leaked_tool_calls(&self, text: &str) -> Vec<ToolCall> {
        let mut recovered = Vec::new();

        const RECOVERY_TEXT_MAX_CHARS: usize = 20_000;
        const RECOVERY_MAX_CALLS: usize = 8;
        if text.len() > RECOVERY_TEXT_MAX_CHARS
            || (!text.contains("[tool_use") && !text.contains("[DUMMY_TOOL"))
        {
            return recovered;
        }

        let tag_re =
            Regex::new(r"(?s)\[tool_use\s+id=([^\s\]>]+)\s+name=([^\s\]>]+)[\]>]\s*(\{.*?\})")
                .unwrap();
        for cap in tag_re.captures_iter(text).take(RECOVERY_MAX_CALLS) {
            recovered.push(ToolCall {
                id: cap[1].to_string(),
                name: cap[2].to_string(),
                arguments: cap[3].to_string(),
            });
        }

        if recovered.is_empty() {
            let json_re =
                Regex::new(r#"(?s)\{\s*"(?:command|file_path|pattern|query)"\s*:.*?\}"#).unwrap();
            for (i, m) in json_re.find_iter(text).take(RECOVERY_MAX_CALLS).enumerate() {
                let json_str = m.as_str();
                let name = if json_str.contains("\"command\"") {
                    "bash"
                } else if json_str.contains("\"file_path\"") && json_str.contains("\"old_string\"")
                {
                    "edit_file"
                } else if json_str.contains("\"file_path\"") {
                    "read_file"
                } else if json_str.contains("\"pattern\"") {
                    "glob"
                } else {
                    "unknown"
                };

                if name != "unknown" {
                    recovered.push(ToolCall {
                        id: format!("recovered_{}", i),
                        name: name.to_string(),
                        arguments: json_str.to_string(),
                    });
                }
            }
        }

        recovered
    }
}
