/// Shell command handling and file references.

use super::{App, ChatEntry, ChatRole};

/// Dangerous command patterns for safety warnings.
const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf",
    "rm -r /",
    "rmdir /",
    "mkfs",
    "dd if=",
    ":(){",           // fork bomb
    "chmod -R 777",
    "chmod -R 000",
    "chown -R",
    "> /dev/sda",
    "wget | sh",
    "curl | sh",
    "curl | bash",
    "wget | bash",
    "shutdown",
    "reboot",
    "init 0",
    "kill -9 -1",
    "pkill -9",
    "DROP TABLE",
    "DROP DATABASE",
    "TRUNCATE",
    "--no-preserve-root",
];

/// Check if a command is potentially dangerous.
pub fn is_dangerous_command(cmd: &str) -> Option<&'static str> {
    let lower = cmd.to_lowercase();
    for pattern in DANGEROUS_PATTERNS {
        if lower.contains(&pattern.to_lowercase()) {
            return Some(pattern);
        }
    }
    None
}

impl App {
    /// Handle ! shell command prefix. Returns true if input was a shell command.
    pub(crate) fn handle_shell_command(&mut self, input: &str) -> bool {
        let trimmed = input.trim();
        if !trimmed.starts_with('!') || trimmed.len() <= 1 {
            return false;
        }

        let cmd = &trimmed[1..];

        // Safety check for dangerous commands
        if let Some(pattern) = is_dangerous_command(cmd) {
            self.chat_entries.push(ChatEntry::new(
                ChatRole::User,
                format!("!{}", cmd),
            ));
            self.add_system_message(format!(
                "⚠ Dangerous command detected: '{}'\nCommand blocked for safety. Use the LLM to execute if intended.",
                pattern
            ));
            return true;
        }

        self.chat_entries.push(ChatEntry::new(
            ChatRole::User,
            format!("!{}", cmd),
        ));

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output();

        let content = match output {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let stderr = String::from_utf8_lossy(&o.stderr);
                let mut result = String::new();
                if !stdout.is_empty() {
                    result.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str(&stderr);
                }
                if result.is_empty() {
                    format!("(exit code: {})", o.status.code().unwrap_or(-1))
                } else {
                    // Truncate very long outputs
                    if result.len() > 10000 {
                        let truncated: String = result.chars().take(10000).collect();
                        format!("{}...\n\n(output truncated, {} total bytes)", truncated, result.len())
                    } else {
                        result
                    }
                }
            }
            Err(e) => format!("Failed to execute: {}", e),
        };

        self.add_system_message(content);
        true
    }

    /// Process @file references in input — attaches file contents.
    pub(crate) fn process_file_references(&self, input: &str) -> String {
        let mut result = input.to_string();
        let mut context_parts: Vec<String> = Vec::new();

        for word in input.split_whitespace() {
            if word.starts_with('@') && word.len() > 1 {
                let file_path = &word[1..];
                match std::fs::read_to_string(file_path) {
                    Ok(content) => {
                        let line_count = content.lines().count();
                        context_parts.push(format!(
                            "\n<file path=\"{}\" lines=\"{}\">\n{}\n</file>",
                            file_path, line_count, content
                        ));
                    }
                    Err(e) => {
                        context_parts.push(format!(
                            "\n<file_error path=\"{}\">{}</file_error>",
                            file_path, e
                        ));
                    }
                }
            }
        }

        if !context_parts.is_empty() {
            result.push_str("\n\n[Attached file context]");
            for part in context_parts {
                result.push_str(&part);
            }
        }
        result
    }

    /// Helper to add a system message.
    pub(crate) fn add_system_message(&mut self, content: String) {
        self.chat_entries.push(ChatEntry::new(ChatRole::System, content));
    }
}

/// Estimate cost based on model with separate input/output pricing (per Mtok).
pub(crate) fn estimate_cost(model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
    let (input_per_mtok, output_per_mtok) = if model.contains("claude-3-opus") || model.contains("claude-opus") {
        (15.0, 75.0)
    } else if model.contains("claude-3-sonnet") || model.contains("claude-3.5") || model.contains("claude-sonnet") {
        (3.0, 15.0)
    } else if model.contains("claude-3-haiku") || model.contains("claude-haiku") {
        (0.25, 1.25)
    } else if model.contains("gpt-4o") {
        (2.5, 10.0)
    } else if model.contains("gpt-4") {
        (30.0, 60.0)
    } else if model.contains("gpt-3.5") {
        (0.5, 1.5)
    } else if model.contains("deepseek") {
        (0.14, 0.28)
    } else {
        (5.0, 15.0)
    };
    (input_tokens as f64 / 1_000_000.0) * input_per_mtok
        + (output_tokens as f64 / 1_000_000.0) * output_per_mtok
}
