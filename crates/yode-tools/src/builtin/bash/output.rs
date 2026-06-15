use super::*;

impl BashTool {
    pub(super) fn format_output(
        &self,
        command: &str,
        working_dir: &Path,
        output: std::process::Output,
        modified_files: Vec<String>,
    ) -> Result<ToolResult> {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        tracing::debug!(
            command = %command,
            exit_code = exit_code,
            stdout_len = stdout.len(),
            stderr_len = stderr.len(),
            "Command completed"
        );

        let mut combined = String::new();

        if !stdout.is_empty() {
            combined.push_str(&stdout);
        }

        if !stderr.is_empty() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str("[stderr]\n");
            combined.push_str(&stderr);
        }

        if !output.status.success() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str(&format!("[exit code: {}]", exit_code));
            return Ok(ToolResult::error(combined));
        }

        let mut metadata = json!({
            "command": command,
            "cwd": working_dir.display().to_string(),
        });
        let cmd_base = command.split_whitespace().next().unwrap_or("");

        let cmd_type = if ["grep", "rg", "find", "ag", "ack"].contains(&cmd_base) {
            "search"
        } else if ["ls", "tree", "du"].contains(&cmd_base) {
            "list"
        } else if ["cat", "head", "tail", "less", "more"].contains(&cmd_base) {
            "read"
        } else {
            "generic"
        };
        metadata["command_type"] = json!(cmd_type);
        let rewrite_suggestion = suggest_safe_rewrite(command, cmd_base);
        if let Some(suggestion) = rewrite_suggestion.as_deref() {
            metadata["rewrite_suggestion"] = json!(suggestion);
        }
        let redirected_files = redirection_targets(command, working_dir);
        let mut changed_files = modified_files;
        for target in redirected_files {
            let display = target.display().to_string();
            if !changed_files.iter().any(|path| path == &display) {
                changed_files.push(display);
            }
        }

        if !changed_files.is_empty() {
            metadata["modified_file_count"] = json!(changed_files.len());
            metadata["modified_files"] = json!(changed_files);
            if let Some(first_file) = changed_files.first() {
                metadata["file_path"] = json!(first_file);
                if let Some(diff_preview) = file_added_preview(first_file) {
                    metadata["diff_preview"] = diff_preview;
                }
            }
        }

        Ok(ToolResult {
            content: combined,
            is_error: false,
            error_type: None,
            recoverable: false,
            suggestion: rewrite_suggestion,
            metadata: Some(metadata),
        })
    }
}

fn redirection_targets(command: &str, working_dir: &Path) -> Vec<std::path::PathBuf> {
    let Ok(re) = Regex::new(r#"(?:^|\s)(?:>|>>)\s*(?:"([^"]+)"|'([^']+)'|([^\s;&|]+))"#) else {
        return Vec::new();
    };
    re.captures_iter(command)
        .filter_map(|captures| {
            captures
                .get(1)
                .or_else(|| captures.get(2))
                .or_else(|| captures.get(3))
                .map(|matched| matched.as_str().trim().to_string())
        })
        .filter(|target| {
            !target.is_empty()
                && !target.starts_with('&')
                && target != "/dev/null"
                && !target.starts_with("/dev/")
        })
        .map(|target| {
            let path = std::path::PathBuf::from(target);
            if path.is_absolute() {
                path
            } else {
                working_dir.join(path)
            }
        })
        .collect()
}

fn file_added_preview(file_path: &str) -> Option<serde_json::Value> {
    let content = std::fs::read_to_string(file_path).ok()?;
    let lines = content.lines().map(str::to_string).collect::<Vec<_>>();
    let line_count = lines.len();
    let preview = lines.into_iter().take(8).collect::<Vec<_>>();
    Some(json!({
        "removed": [],
        "added": preview,
        "more_removed": 0,
        "more_added": line_count.saturating_sub(8),
    }))
}

fn suggest_safe_rewrite(command: &str, cmd_base: &str) -> Option<String> {
    if ["grep", "rg", "find", "ag", "ack"].contains(&cmd_base) {
        return Some(
            "Prefer `grep` or `glob` tools for search work so results stay structured and reviewable."
                .to_string(),
        );
    }
    if ["cat", "head", "tail", "less", "more"].contains(&cmd_base) {
        return Some(
            "Prefer `read_file` for file reads so the agent keeps precise file context."
                .to_string(),
        );
    }
    if ["sed", "awk"].contains(&cmd_base) {
        return Some(
            "Prefer `edit_file` for text edits so replacements are validated and diff-aware."
                .to_string(),
        );
    }
    if ["echo", "printf"].contains(&cmd_base) && (command.contains(" >") || command.contains(" >>"))
    {
        return Some(
            "Prefer `write_file` for file creation/overwrite instead of shell redirection."
                .to_string(),
        );
    }
    None
}

pub(super) fn looks_like_interactive_prompt(tail: &str) -> bool {
    let trimmed = tail.trim_end();
    if trimmed.is_empty() {
        return false;
    }

    let last_line = trimmed.lines().last().unwrap_or("");
    for pattern in INTERACTIVE_PROMPT_PATTERNS {
        if last_line.contains(pattern) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_interactive_prompt() {
        assert!(looks_like_interactive_prompt("Enter password: "));
        assert!(looks_like_interactive_prompt("Continue? [y/n] "));
        assert!(looks_like_interactive_prompt(
            "Are you sure you want to proceed?"
        ));
        assert!(looks_like_interactive_prompt("Username: "));
        assert!(!looks_like_interactive_prompt(
            "Build completed successfully"
        ));
        assert!(!looks_like_interactive_prompt(""));
        assert!(!looks_like_interactive_prompt("  \n  \n"));
    }

    #[test]
    fn test_suggest_safe_rewrite_detects_better_tool_paths() {
        assert!(suggest_safe_rewrite("grep -R foo src", "grep")
            .unwrap()
            .contains("grep"));
        assert!(suggest_safe_rewrite("cat Cargo.toml", "cat")
            .unwrap()
            .contains("read_file"));
        assert!(suggest_safe_rewrite("sed -i '' 's/a/b/' file.txt", "sed")
            .unwrap()
            .contains("edit_file"));
        assert!(suggest_safe_rewrite("echo hi > out.txt", "echo")
            .unwrap()
            .contains("write_file"));
        assert!(suggest_safe_rewrite("git status", "git").is_none());
    }
}
