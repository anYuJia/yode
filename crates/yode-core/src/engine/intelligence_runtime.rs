use super::*;

impl AgentEngine {
    pub(super) fn inject_intelligence(
        &mut self,
        result: &mut ToolResult,
        tool_name: &str,
        tool_args: &str,
    ) {
        self.tool_call_count += 1;

        if result.is_error {
            self.consecutive_failures += 1;

            if let Some(err_type) = &result.error_type {
                let err_type_val = *err_type;
                {
                    let count = self.error_buckets.entry(err_type_val).or_insert(0);
                    *count += 1;
                }
                let bucket_count = *self.error_buckets.get(&err_type_val).unwrap_or(&0);

                let current_sig = format!("{}:{}", tool_name, tool_args);
                let is_exact_retry = self.last_failed_signature.as_ref() == Some(&current_sig);
                self.last_failed_signature = Some(current_sig);

                self.update_recovery_state();
                match (err_type_val, bucket_count) {
                    (ToolErrorType::NotFound, c) if c >= 2 => {
                        result.content.push_str("\n\n[CRITICAL STRATEGY CHANGE: You have failed to find paths multiple times. STOP assuming paths. You MUST run `ls` on the parent directory or use `glob` to re-anchor your workspace understanding before trying this path again.]");
                    }
                    (ToolErrorType::Validation, c) if c >= 2 || is_exact_retry => {
                        result.content.push_str("\n\n[CRITICAL STRATEGY CHANGE: Your tool parameters are repeatedly invalid. Read the tool definition carefully and check for typos in file names or JSON structure. Do NOT repeat the same parameters.]");
                    }
                    (ToolErrorType::Protocol, c) if c >= 2 => {
                        result.content.push_str("\n\n[CRITICAL STRATEGY CHANGE: You are repeatedly outputting internal tool tags in your text. This is a protocol violation. STOP using square brackets or tags like `[tool_use]` in your text response. Use ONLY natural language for text and the structured tool calling interface for tools.]");
                    }
                    (ToolErrorType::Timeout, c) if c >= 2 => {
                        result.content.push_str("\n\n[CRITICAL STRATEGY CHANGE: Operations are timing out. The scope is too large. Break your task into much smaller steps or use more specific search patterns.]");
                    }
                    _ => {
                        if result.suggestion.is_none() && bucket_count == 1 {
                            let hint = match err_type_val {
                                ToolErrorType::NotFound => "Hint: Use `glob` or `ls` to verify the path exists.",
                                ToolErrorType::Validation => "Hint: Check the required parameters and types in the tool schema.",
                                _ => "Hint: Try a different approach or tool.",
                            };
                            result.content.push_str(&format!("\n\n{}", hint));
                        }
                    }
                }
            }
        } else {
            self.consecutive_failures = 0;
            self.error_buckets.clear();
            self.last_failed_signature = None;
            self.violation_retries = 0;

            let is_discovery_tool =
                matches!(tool_name, "ls" | "glob" | "read_file" | "project_map");
            if self.recovery_state == RecoveryState::ReanchorRequired && is_discovery_tool {
                self.consecutive_failures = 0;
                self.error_buckets.clear();
                self.last_failed_signature = None;
                self.recovery_state = RecoveryState::Normal;
                result.content.push_str(
                    "\n\n[Recovery: Workspace re-anchored successfully. Normal tool execution is now resumed.]",
                );
            } else {
                self.consecutive_failures = 0;
                self.error_buckets.clear();
                self.last_failed_signature = None;
                self.update_recovery_state();
            }
        }

        let file_path = serde_json::from_str::<serde_json::Value>(tool_args)
            .ok()
            .and_then(|v| {
                v.get("file_path")
                    .and_then(|p| p.as_str())
                    .map(String::from)
            });

        if let Some(ref path) = file_path {
            match tool_name {
                "read_file" if !result.is_error => {
                    let line_count = result.content.lines().count();
                    if let Some(&prev_lines) = self.files_read.get(path.as_str()) {
                        result.content.push_str(&format!(
                            "\n\n[Note: You already read this file earlier ({} lines). \
                             If you need specific lines, use offset/limit instead of re-reading.]",
                            prev_lines
                        ));
                    }
                    self.files_read.insert(path.clone(), line_count);
                }
                "edit_file" | "write_file" | "multi_edit" if !result.is_error => {
                    self.files_modified.push(path.clone());
                }
                _ => {}
            }
        }

        if !result.is_error && (tool_name == "edit_file" || tool_name == "write_file") {
            if self.files_modified.len() == 1 {
                result.content.push_str(
                    "\n\n[Next: Run `bash` with build command to verify. \
                     If you changed a function signature, grep for callers to update them too.]",
                );
            } else if self.files_modified.len() > 3 {
                result.content.push_str(&format!(
                    "\n\n[You've modified {} files so far. Run a build to catch any issues before continuing.]",
                    self.files_modified.len()
                ));
            }
        }

        if tool_name == "bash" && result.is_error {
            if result.content.contains("error[E") {
                if let Some(line) = result.content.lines().find(|l| l.contains("error[E")) {
                    result.content.push_str(&format!(
                        "\n\n[Build error detected. Focus on the first error: `{}`\n\
                         Read the file at the indicated line to understand the issue before attempting a fix.]",
                        line.trim().chars().take(200).collect::<String>()
                    ));
                }
            }
        }

        if self.consecutive_failures == 2 {
            result.content.push_str(
                "\n\n[2 failures in a row. Your current approach isn't working. \
                 Step back: What assumption might be wrong? Try a different tool or strategy.]",
            );
        } else if self.consecutive_failures >= 3 {
            result.content.push_str(
                "\n\n[3+ consecutive failures. STOP searching and summarize what you know. \
                 Present your findings to the user and ask for guidance.]",
            );
        }

        if self.tool_call_count == TOOL_GOAL_REMINDER {
            result.content.push_str(
                "\n\n[5 tool calls done. Quick check: Do you have enough information to act? \
                 If yes, stop gathering and start implementing.]",
            );
        }

        if self.tool_call_count > 0 && self.tool_call_count.is_multiple_of(TOOL_REFLECT_INTERVAL) {
            let state_summary = format!(
                "\n\n[Checkpoint: {} tool calls | {} files read | {} files modified. \
                 Summarize your understanding. What's your hypothesis? What's the most efficient next step?]",
                self.tool_call_count,
                self.files_read.len(),
                self.files_modified.len()
            );
            result.content.push_str(&state_summary);
        }

        if let Some(message) = self.maybe_record_tool_budget_warning() {
            result.content.push_str(&format!("\n\n[{}]", message));
        }
    }

    /// Generate a prompt suggestion using LLM.
    pub async fn generate_prompt_suggestion(&self, recent_messages: &[Message]) -> Option<String> {
        let suggestion_prompt = r#"[SUGGESTION MODE: Suggest what the user might naturally type next.]

FIRST: Look at the user's recent messages and original request.

Your job is to predict what THEY would type - not what you think they should do.

THE TEST: Would they think "I was just about to type that"?

EXAMPLES:
- User asked "fix the bug and run tests", bug is fixed -> "run the tests"
- After code written -> "try it out"
- Claude offers options -> suggest the one the user would likely pick
- Claude asks to continue -> "yes" or "go ahead"
- Task complete, obvious follow-up -> "commit this" or "push it"

Be specific: "run the tests" beats "continue".

NEVER SUGGEST:
- Evaluative ("looks good", "thanks")
- Questions ("what about...?")
- Claude-voice ("Let me...", "I'll...", "Here's...")
- New ideas they didn't ask about
- Multiple sentences

Stay silent if the next step isn't obvious from what the user said.

Format: 2-12 words, match the user's style. Or nothing.

Reply with ONLY the suggestion, no quotes or explanation."#;

        let mut messages = vec![Message::system(suggestion_prompt)];

        let context_start = recent_messages.len().saturating_sub(6);
        for msg in &recent_messages[context_start..] {
            if let Some(ref content) = msg.content {
                if !content.trim().is_empty() {
                    messages.push(msg.clone());
                }
            }
        }

        let request = ChatRequest {
            model: self.context.model.clone(),
            messages,
            tools: vec![],
            temperature: Some(0.7),
            max_tokens: Some(50),
        };

        let provider = Arc::clone(&self.provider);

        match tokio::time::timeout(std::time::Duration::from_secs(5), provider.chat(request)).await
        {
            Ok(Ok(response)) => {
                if let Some(content) = response.message.content {
                    let suggestion = content.trim().to_string();
                    if !suggestion.is_empty()
                        && suggestion.len() <= 100
                        && !suggestion.starts_with('[')
                        && !suggestion.contains("silence")
                    {
                        return Some(suggestion);
                    }
                }
            }
            Ok(Err(e)) => {
                debug!("Prompt suggestion generation failed: {}", e);
            }
            Err(_) => {
                tracing::trace!("Prompt suggestion generation timed out (expected for slow APIs)");
            }
        }

        None
    }
}
