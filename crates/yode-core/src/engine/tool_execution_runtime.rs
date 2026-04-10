use super::*;

use crate::tool_runtime::ToolResultTruncationView;

impl AgentEngine {
    /// Partition tool calls into (parallel, sequential) based on permission and read_only.
    pub(super) fn partition_tool_calls(&self, tool_calls: &[ToolCall]) -> (Vec<ToolCall>, Vec<ToolCall>) {
        let mut parallel = Vec::new();
        let mut sequential = Vec::new();

        if self.recovery_state != RecoveryState::Normal {
            return (parallel, tool_calls.to_vec());
        }

        for tc in tool_calls {
            let can_parallel = if let Some(tool) = self.tools.get(&tc.name) {
                let caps = tool.capabilities();
                caps.read_only
                    && matches!(self.permissions.check(&tc.name), PermissionAction::Allow)
            } else {
                false
            };

            if can_parallel {
                parallel.push(tc.clone());
            } else {
                sequential.push(tc.clone());
            }
        }

        (parallel, sequential)
    }

    /// Execute a batch of read-only, auto-allowed tool calls in parallel.
    pub(super) async fn execute_tools_parallel(
        &mut self,
        tool_calls: &[ToolCall],
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> Vec<ToolExecutionOutcome> {
        use futures::future::join_all;

        let mut futures = Vec::new();
        let batch_id = self.register_parallel_batch(tool_calls.len());

        for tc in tool_calls {
            let tool = match self.tools.get(&tc.name) {
                Some(t) => t,
                None => continue,
            };

            let mut params: serde_json::Value = serde_json::from_str(&tc.arguments)
                .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
            let working_dir = self.current_runtime_working_dir().await;

            if let Some(blocked) = self
                .run_pre_tool_use_hook(&tc.name, &tc.arguments, &working_dir, &mut params)
                .await
            {
                let tc_clone = tc.clone();
                futures.push(Box::pin(async move {
                    ToolExecutionOutcome {
                        tool_call: tc_clone,
                        result: blocked,
                        started_at: Some(Self::now_timestamp()),
                        duration_ms: 0,
                        progress_updates: 0,
                        last_progress_message: None,
                        parallel_batch: Some(batch_id),
                    }
                })
                    as Pin<
                        Box<dyn std::future::Future<Output = ToolExecutionOutcome> + Send>,
                    >);
                continue;
            }

            let schema = tool.parameters_schema();
            if let Err(msg) = validation::validate_and_coerce(&schema, &mut params) {
                let tc_clone = tc.clone();
                let result = ToolResult::error_typed(
                    format!("Parameter validation failed: {}", msg),
                    ToolErrorType::Validation,
                    true,
                    Some(format!("Fix the parameters and retry. Schema: {}", schema)),
                );
                futures.push(Box::pin(async move {
                    ToolExecutionOutcome {
                        tool_call: tc_clone,
                        result,
                        started_at: Some(Self::now_timestamp()),
                        duration_ms: 0,
                        progress_updates: 0,
                        last_progress_message: None,
                        parallel_batch: Some(batch_id),
                    }
                })
                    as Pin<
                        Box<dyn std::future::Future<Output = ToolExecutionOutcome> + Send>,
                    >);
                continue;
            }
            let effective_arguments =
                serde_json::to_string(&params).unwrap_or_else(|_| tc.arguments.clone());

            let _ = event_tx.send(EngineEvent::ToolCallStart {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: effective_arguments,
            });

            info!(
                "Executing tool in parallel: {} (auto-allowed, read-only)",
                tc.name
            );

            let (p_tx, mut p_rx) = mpsc::unbounded_channel::<yode_tools::tool::ToolProgress>();
            let event_tx_inner = event_tx.clone();
            let tc_id = tc.id.clone();
            let tc_name = tc.name.clone();
            let progress_counter = Arc::new(AtomicU64::new(0));
            let progress_counter_inner = Arc::clone(&progress_counter);
            let last_progress_message = Arc::new(std::sync::Mutex::new(None::<String>));
            let last_progress_message_inner = Arc::clone(&last_progress_message);
            tokio::spawn(async move {
                while let Some(progress) = p_rx.recv().await {
                    progress_counter_inner.fetch_add(1, Ordering::Relaxed);
                    if let Ok(mut slot) = last_progress_message_inner.lock() {
                        *slot = Some(progress.message.clone());
                    }
                    let _ = event_tx_inner.send(EngineEvent::ToolProgress {
                        id: tc_id.clone(),
                        name: tc_name.clone(),
                        progress,
                    });
                }
            });

            let ctx = self.build_tool_context(Some(p_tx)).await;

            let tool_name = tc.name.clone();
            let tc_clone = tc.clone();
            let started_at = Some(Self::now_timestamp());

            futures.push(Box::pin(async move {
                let start = std::time::Instant::now();
                let timeout = std::time::Duration::from_secs(PARALLEL_TOOL_TIMEOUT_SECS);
                let result = match tokio::time::timeout(timeout, tool.execute(params, &ctx)).await {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => {
                        error!("Tool {} failed: {}", tool_name, e);
                        ToolResult::error(format!("Tool execution failed: {}", e))
                    }
                    Err(_) => {
                        warn!(
                            "Tool {} timed out after {}s",
                            tool_name, PARALLEL_TOOL_TIMEOUT_SECS
                        );
                        ToolResult::error_typed(
                            format!(
                                "Tool {} timed out after {} seconds",
                                tool_name, PARALLEL_TOOL_TIMEOUT_SECS
                            ),
                            ToolErrorType::Timeout,
                            true,
                            Some("Try a smaller scope or more specific parameters.".to_string()),
                        )
                    }
                };
                debug!(
                    tool = %tool_name,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "Parallel tool completed"
                );
                let progress_updates = progress_counter.load(Ordering::Relaxed) as u32;
                let last_progress_message = last_progress_message
                    .lock()
                    .ok()
                    .and_then(|slot| slot.clone());
                ToolExecutionOutcome {
                    tool_call: tc_clone,
                    result,
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    progress_updates,
                    last_progress_message,
                    parallel_batch: Some(batch_id),
                }
            }));
        }

        let outcomes = join_all(futures).await;
        for outcome in &outcomes {
            self.record_tool_progress_summary(
                &outcome.tool_call.name,
                outcome.progress_updates,
                outcome.last_progress_message.clone(),
            );
        }
        outcomes
    }

    /// Tracks file read/modified status from tool results.
    fn track_file_access(&mut self, tool_name: &str, result: &ToolResult) {
        if result.is_error {
            return;
        }

        if let Some(ref metadata) = result.metadata {
            if let Some(new_cwd) = metadata.get("cwd").and_then(|v| v.as_str()) {
                let runtime = self.context.runtime.clone();
                let new_path = std::path::PathBuf::from(new_cwd);
                tokio::spawn(async move {
                    let mut rt = runtime.lock().await;
                    if rt.cwd != new_path {
                        debug!("Syncing session CWD to: {}", new_path.display());
                        rt.cwd = new_path.clone();
                        rt.last_success_cwd = new_path;
                    }
                });
            }

            if let Some(file_path) = metadata.get("file_path").and_then(|v| v.as_str()) {
                match tool_name {
                    "read_file" => {
                        let lines = metadata
                            .get("total_lines")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
                        self.files_read.insert(file_path.to_string(), lines);
                    }
                    "edit_file" | "write_file" | "multi_edit" | "notebook_edit" => {
                        self.files_modified.push(file_path.to_string());
                    }
                    _ => {}
                }
            }
        }
    }

    /// Cleans hallucinated protocol tags from LLM text response.
    pub(super) fn clean_assistant_response(&self, text: &str) -> String {
        let re = Regex::new(r"(?s)\[DUMMY_TOOL_RESULT\]|\[tool_use\s+[^\]>]+[\]>](?:\s*[:]\s*)?\{.*?\}[\s\]>]*|\[tool_result\s+[^\]>]+[\]>](?:\s*[:]\s*)?\{.*?\}[\s\]>]*|\[tool_(?:use|result)\s+[^\]>]+[\]>]").unwrap();
        re.replace_all(text, "").to_string()
    }

    /// Detects if the assistant response contains forbidden internal protocol patterns.
    pub(super) fn is_protocol_violation(&self, text: &str) -> bool {
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
    pub(super) fn recover_leaked_tool_calls(&self, text: &str) -> Vec<ToolCall> {
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

    /// Enforces per-turn aggregate tool result size limits.
    fn enforce_tool_budget(&mut self, result: &mut ToolResult) {
        let size = result.content.len();
        self.total_tool_results_bytes += size;

        if self.total_tool_results_bytes > MAX_TOTAL_TOOL_RESULTS_SIZE {
            let over_limit = self.total_tool_results_bytes - MAX_TOTAL_TOOL_RESULTS_SIZE;
            if size > over_limit {
                let allowed = size.saturating_sub(over_limit);
                let preview_len = allowed.min(1000);
                let preview: String = result.content.chars().take(preview_len).collect();
                let original_bytes = size;

                result.content = format!(
                    "{}\n\n[AGGREGATE BUDGET EXCEEDED: Remaining {} bytes of this result omitted. \
                     STOP requesting large outputs in this turn to avoid context overflow.]",
                    preview,
                    size - preview_len
                );
                set_tool_runtime_truncation_metadata(
                    result,
                    &ToolResultTruncationView {
                        reason: "aggregate_budget_partial".to_string(),
                        original_bytes,
                        kept_bytes: result.content.len(),
                        omitted_bytes: original_bytes.saturating_sub(result.content.len()),
                    },
                );
            } else {
                let original_bytes = size;
                result.content = format!(
                    "[AGGREGATE BUDGET EXCEEDED: Full result ({} bytes) omitted to prevent context overflow. \
                     Summarize your current findings instead.]",
                    size
                );
                set_tool_runtime_truncation_metadata(
                    result,
                    &ToolResultTruncationView {
                        reason: "aggregate_budget_omitted".to_string(),
                        original_bytes,
                        kept_bytes: result.content.len(),
                        omitted_bytes: original_bytes.saturating_sub(result.content.len()),
                    },
                );
            }
        }
    }

    pub(super) async fn finalize_tool_result(
        &mut self,
        tool_call: &ToolCall,
        mut result: ToolResult,
        started_at: Option<String>,
        duration_ms: u64,
        progress_updates: u32,
        parallel_batch: Option<u32>,
    ) -> ToolResult {
        self.track_file_access(&tool_call.name, &result);
        result = truncate_tool_result(result);
        self.enforce_tool_budget(&mut result);
        self.inject_intelligence(&mut result, &tool_call.name, &tool_call.arguments);

        let working_dir = self.current_runtime_working_dir().await;
        let effective_input = Self::parse_tool_input(&tool_call.arguments);
        self.run_post_tool_use_hooks(tool_call, &effective_input, &working_dir, &mut result)
            .await;
        annotate_tool_result_runtime_metadata(
            &mut result,
            duration_ms,
            progress_updates,
            parallel_batch,
            tool_call.arguments.len(),
        );

        self.record_tool_execution_trace(
            tool_call,
            &result,
            started_at,
            duration_ms,
            progress_updates,
            parallel_batch,
            tool_call.arguments.len(),
        );
        self.record_tool_result_status(&tool_call.id, &result);
        result
    }

    fn immediate_tool_outcome(
        tool_call: &ToolCall,
        started_at: &Option<String>,
        result: ToolResult,
    ) -> ToolExecutionOutcome {
        ToolExecutionOutcome {
            tool_call: tool_call.clone(),
            result,
            started_at: started_at.clone(),
            duration_ms: 0,
            progress_updates: 0,
            last_progress_message: None,
            parallel_batch: None,
        }
    }

    /// Handle a single tool call...
    pub(super) async fn handle_tool_call(
        &mut self,
        tool_call: &ToolCall,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        confirm_rx: &mut mpsc::UnboundedReceiver<ConfirmResponse>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<ToolExecutionOutcome> {
        let started_at = Some(Self::now_timestamp());
        let tool = match self.tools.get(&tool_call.name) {
            Some(t) => t,
            None => {
                return Ok(ToolExecutionOutcome {
                    tool_call: tool_call.clone(),
                    result: ToolResult::error(format!("Unknown tool: {}", tool_call.name)),
                    started_at,
                    duration_ms: 0,
                    progress_updates: 0,
                    last_progress_message: None,
                    parallel_batch: None,
                });
            }
        };

        let original_params: serde_json::Value = serde_json::from_str(&tool_call.arguments)
            .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
        let mut params = original_params.clone();
        let working_dir = self.current_runtime_working_dir().await;

        if let Some(blocked) = self
            .run_pre_tool_use_hook(
                &tool_call.name,
                &tool_call.arguments,
                &working_dir,
                &mut params,
            )
            .await
        {
            return Ok(ToolExecutionOutcome {
                tool_call: tool_call.clone(),
                result: blocked,
                started_at,
                duration_ms: 0,
                progress_updates: 0,
                last_progress_message: None,
                parallel_batch: None,
            });
        }

        if self.recovery_state == RecoveryState::ReanchorRequired {
            let allow_reanchor_tool = matches!(
                tool_call.name.as_str(),
                "ls" | "glob" | "read_file" | "project_map"
            );
            if !allow_reanchor_tool {
                return Ok(ToolExecutionOutcome {
                    tool_call: tool_call.clone(),
                    result: ToolResult::error_typed(
                        format!(
                            "Recovery gate active: '{}' is temporarily blocked until workspace is re-anchored.",
                            tool_call.name
                        ),
                        ToolErrorType::Validation,
                        true,
                        Some(
                            "Run a lightweight discovery step first (ls/glob/read_file/project_map), then continue with execution tools."
                                .to_string(),
                        ),
                    ),
                    started_at,
                    duration_ms: 0,
                    progress_updates: 0,
                    last_progress_message: None,
                    parallel_batch: None,
                });
            }
        }

        if let Some(file_path) = params.get("file_path").and_then(|v| v.as_str()) {
            let mut reason = None;
            if file_path.contains("..") {
                reason = Some("Path traversal (..) is strictly forbidden for security reasons.");
            } else if file_path.contains('$') || file_path.contains('%') {
                reason = Some("Unexpanded shell variables ($VAR, %VAR%) are not allowed in paths. Use absolute or relative literal paths.");
            } else if file_path.starts_with('~') {
                reason = Some("Tilde (~) is not expanded. Use the full absolute path or a path relative to the current working directory.");
            }

            if let Some(r) = reason {
                return Ok(Self::immediate_tool_outcome(
                    tool_call,
                    &started_at,
                    ToolResult::error_typed(
                        format!("Security Block: '{}' is an invalid path. {}", file_path, r),
                        ToolErrorType::Validation,
                        true,
                        Some(
                            "Correct the path to a literal, normalized format and try again."
                                .to_string(),
                        ),
                    ),
                ));
            }
        }

        let command_content = if tool_call.name == "bash" {
            params
                .get("command")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };
        let effective_arguments =
            serde_json::to_string(&params).unwrap_or_else(|_| tool_call.arguments.clone());
        let input_changed_by_hook = params != original_params;

        if let Some(reason) = self.language_command_mismatch(&tool_call.name, &params) {
            return Ok(Self::immediate_tool_outcome(
                tool_call,
                &started_at,
                ToolResult::error_typed(
                    format!("Command blocked by project gate: {}", reason),
                    ToolErrorType::Validation,
                    true,
                    Some("Re-anchor with ls/glob/read on the target project root, then run matching build tooling.".to_string()),
                ),
            ));
        }

        if tool_call.name == "edit_file" || tool_call.name == "write_file" {
            if let Some(file_path) = params.get("file_path").and_then(|v| v.as_str()) {
                if !self.files_read.contains_key(file_path) {
                    return Ok(Self::immediate_tool_outcome(
                        tool_call,
                        &started_at,
                        ToolResult::error_typed(
                            format!("You must read the file '{}' with read_file before editing or overwriting it.", file_path),
                            ToolErrorType::Validation,
                            true,
                            Some(format!("Call read_file(file_path=\"{}\") first.", file_path)),
                        ),
                    ));
                }
            }
        }

        let permission_explanation = self
            .permissions
            .explain_with_content(&tool_call.name, command_content.as_deref());
        self.last_permission_tool = Some(tool_call.name.clone());
        self.last_permission_action = Some(permission_explanation.action.label().to_string());
        self.last_permission_explanation = Some(permission_explanation.reason.clone());
        self.write_permission_artifact(
            "permission_manager",
            &tool_call.name,
            permission_explanation.action.label(),
            &permission_explanation.reason,
            &params,
            &effective_arguments,
            &original_params,
            &tool_call.arguments,
            input_changed_by_hook,
        );
        let action = permission_explanation.action.clone();

        if tool_call.name == "bash" {
            if let Some(ref cmd) = command_content {
                let cmd_lower = cmd.to_lowercase();
                let forbidden_binaries = ["find", "grep", "rg", "ag", "ack"];
                let is_forbidden = forbidden_binaries.iter().any(|&bin| {
                    let pattern = format!(r"(\s|^|&&|;|\|){}(\s|$)", bin);
                    if let Ok(re) = Regex::new(&pattern) {
                        re.is_match(&cmd_lower)
                    } else {
                        false
                    }
                });

                let is_recursive_ls = cmd_lower.contains("ls ")
                    && (cmd_lower.contains("-r") || cmd_lower.contains("-lar"));

                if is_forbidden || is_recursive_ls {
                    let (cmd_name, alternative) = if is_forbidden {
                        let matched = forbidden_binaries
                            .iter()
                            .find(|&&b| cmd_lower.contains(b))
                            .unwrap_or(&"search");
                        (
                            *matched,
                            match *matched {
                                "find" => "glob",
                                _ => "grep",
                            },
                        )
                    } else {
                        ("ls -R", "ls (without -R) or project_map")
                    };

                    return Ok(Self::immediate_tool_outcome(
                        tool_call,
                        &started_at,
                        ToolResult::error_typed(
                            format!("Command blocked: Use the dedicated '{}' tool instead of running '{}' via bash.", alternative, cmd_name),
                            ToolErrorType::Validation,
                            true,
                            Some(format!("Running search/discovery via bash is inefficient. Use the '{}' tool for better results and TUI display.", alternative)),
                        ),
                    ));
                }

                if CommandClassifier::classify(cmd) == CommandRiskLevel::Destructive {
                    self.last_permission_action = Some("deny".to_string());
                    self.last_permission_explanation = Some(
                        "Dangerous bash command blocked by destructive-command guard. Use a safer non-destructive probe first."
                            .to_string(),
                    );
                    self.write_permission_artifact(
                        "destructive_guard",
                        &tool_call.name,
                        "deny",
                        "Dangerous bash command blocked by destructive-command guard. Use a safer non-destructive probe first.",
                        &params,
                        &effective_arguments,
                        &original_params,
                        &tool_call.arguments,
                        input_changed_by_hook,
                    );
                    return Ok(Self::immediate_tool_outcome(
                        tool_call,
                        &started_at,
                        ToolResult::error_typed(
                            format!("Command blocked (destructive): {}", cmd),
                            ToolErrorType::PermissionDeny,
                            false,
                            Some(
                                "This command is classified as destructive and cannot be executed. Stop and propose a safer fallback such as `git status`, `git diff`, `ls`, or a dry-run variant before attempting any mutation again."
                                    .to_string(),
                            ),
                        ),
                    ));
                }
            }
        }

        match action {
            PermissionAction::Allow => {
                info!("Executing tool: {} (auto-allowed)", tool_call.name);
                let _ = event_tx.send(EngineEvent::ToolCallStart {
                    id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    arguments: effective_arguments.clone(),
                });
            }
            PermissionAction::Confirm => {
                let permission_request_ctx = HookContext {
                    event: HookEvent::PermissionRequest.to_string(),
                    session_id: self.context.session_id.clone(),
                    working_dir: self.context.working_dir_compat().display().to_string(),
                    tool_name: Some(tool_call.name.clone()),
                    tool_input: Some(params.clone()),
                    tool_output: None,
                    error: None,
                    user_prompt: None,
                    metadata: Some(json!({
                        "decision": "confirm",
                        "effective_input_snapshot": params.clone(),
                        "effective_arguments_snapshot": effective_arguments.clone(),
                        "original_input_snapshot": original_params.clone(),
                        "original_arguments_snapshot": tool_call.arguments.clone(),
                        "input_changed_by_hook": input_changed_by_hook,
                    })),
                };
                self.execute_advisory_hooks(HookEvent::PermissionRequest, permission_request_ctx)
                    .await;

                let _ = event_tx.send(EngineEvent::ToolConfirmRequired {
                    id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    arguments: effective_arguments.clone(),
                });

                debug!("Waiting for user confirmation: tool={}", tool_call.name);
                let confirm_start = std::time::Instant::now();
                let confirm_timeout = std::time::Duration::from_secs(90);
                loop {
                    if confirm_start.elapsed() > confirm_timeout {
                        return Ok(Self::immediate_tool_outcome(
                            tool_call,
                            &started_at,
                            ToolResult::error_typed(
                                format!("Confirmation timed out for tool '{}'", tool_call.name),
                                ToolErrorType::Timeout,
                                true,
                                Some("No confirmation was received within 90s. Re-run or switch to a read-only alternative.".to_string()),
                            ),
                        ));
                    }

                    if let Some(token) = cancel_token {
                        if token.is_cancelled() {
                            return Ok(Self::immediate_tool_outcome(
                                tool_call,
                                &started_at,
                                ToolResult::error_typed(
                                    format!("Tool confirmation cancelled: {}", tool_call.name),
                                    ToolErrorType::Timeout,
                                    true,
                                    Some(
                                        "User cancelled while waiting for confirmation."
                                            .to_string(),
                                    ),
                                ),
                            ));
                        }
                    }

                    match tokio::time::timeout(
                        std::time::Duration::from_millis(500),
                        confirm_rx.recv(),
                    )
                    .await
                    {
                        Ok(Some(ConfirmResponse::Allow)) => {
                            info!("Tool {} confirmed by user", tool_call.name);
                            break;
                        }
                        Ok(Some(ConfirmResponse::Deny)) => {
                            info!("Tool {} denied by user", tool_call.name);
                            self.permissions.record_denial(&tool_call.name);
                            self.write_permission_artifact(
                                "user_confirmation",
                                &tool_call.name,
                                "deny",
                                "Tool execution denied by user.",
                                &params,
                                &effective_arguments,
                                &original_params,
                                &tool_call.arguments,
                                input_changed_by_hook,
                            );
                            let denied_ctx = HookContext {
                                event: HookEvent::PermissionDenied.to_string(),
                                session_id: self.context.session_id.clone(),
                                working_dir: self
                                    .context
                                    .working_dir_compat()
                                    .display()
                                    .to_string(),
                                tool_name: Some(tool_call.name.clone()),
                                tool_input: Some(params.clone()),
                                tool_output: None,
                                error: Some("Tool execution denied by user.".to_string()),
                                user_prompt: None,
                                metadata: Some(json!({
                                    "source": "user_confirmation",
                                    "effective_input_snapshot": params.clone(),
                                    "effective_arguments_snapshot": effective_arguments.clone(),
                                    "original_input_snapshot": original_params.clone(),
                                    "original_arguments_snapshot": tool_call.arguments.clone(),
                                    "input_changed_by_hook": input_changed_by_hook,
                                })),
                            };
                            self.execute_advisory_hooks(HookEvent::PermissionDenied, denied_ctx)
                                .await;
                            return Ok(Self::immediate_tool_outcome(
                                tool_call,
                                &started_at,
                                ToolResult::error("Tool execution denied by user.".to_string()),
                            ));
                        }
                        Ok(None) => {
                            return Ok(Self::immediate_tool_outcome(
                                tool_call,
                                &started_at,
                                ToolResult::error_typed(
                                    "Confirmation channel closed.".to_string(),
                                    ToolErrorType::Execution,
                                    true,
                                    Some("Please retry the action. If this repeats, check TUI confirmation event handling.".to_string()),
                                ),
                            ));
                        }
                        Err(_) => {}
                    }
                }
            }
            PermissionAction::Deny => {
                let denied_ctx = HookContext {
                    event: HookEvent::PermissionDenied.to_string(),
                    session_id: self.context.session_id.clone(),
                    working_dir: self.context.working_dir_compat().display().to_string(),
                    tool_name: Some(tool_call.name.clone()),
                    tool_input: Some(params.clone()),
                    tool_output: None,
                    error: Some(format!("Tool {} is not permitted.", tool_call.name)),
                    user_prompt: None,
                    metadata: Some(json!({
                        "source": "permission_manager",
                        "effective_input_snapshot": params.clone(),
                        "effective_arguments_snapshot": effective_arguments.clone(),
                        "original_input_snapshot": original_params.clone(),
                        "original_arguments_snapshot": tool_call.arguments.clone(),
                        "input_changed_by_hook": input_changed_by_hook,
                    })),
                };
                self.execute_advisory_hooks(HookEvent::PermissionDenied, denied_ctx)
                    .await;
                return Ok(Self::immediate_tool_outcome(
                    tool_call,
                    &started_at,
                    ToolResult::error_typed(
                        format!(
                            "Tool {} is not permitted. {}",
                            tool_call.name, permission_explanation.reason
                        ),
                        ToolErrorType::PermissionDeny,
                        false,
                        Some(
                            "Use a safer read-only tool first, or switch permission mode / rules explicitly before retrying."
                                .to_string(),
                        ),
                    ),
                ));
            }
        }

        let call_sig = (tool_call.name.clone(), effective_arguments.clone());
        let current_sig_text = format!("{}:{}", tool_call.name, effective_arguments);

        let is_observer_tool = [
            "ls",
            "glob",
            "grep",
            "git_status",
            "git_diff",
            "git_log",
            "project_map",
            "todo",
            "read_file",
        ]
        .contains(&tool_call.name.as_str());

        if !is_observer_tool
            && self.consecutive_failures >= 2
            && self.last_failed_signature.as_ref() == Some(&current_sig_text)
        {
            return Ok(Self::immediate_tool_outcome(
                tool_call,
                &started_at,
                ToolResult::error_typed(
                    format!(
                        "Blocked repeated failing call: {} is being retried with identical arguments after multiple failures.",
                        tool_call.name
                    ),
                    ToolErrorType::Validation,
                    true,
                    Some("Do not retry the same call. Re-anchor first (ls/glob/read), then change tool arguments.".to_string()),
                ),
            ));
        }

        if self.recent_tool_calls.contains(&call_sig) && !is_observer_tool {
            return Ok(Self::immediate_tool_outcome(
                tool_call,
                &started_at,
                ToolResult::error_typed(
                    format!(
                        "Duplicate tool call detected: {} was called with identical arguments recently. \
                         If you are stuck, try a different approach, search for more information, or summarize your progress.",
                        tool_call.name
                    ),
                    ToolErrorType::Validation,
                    true,
                    Some("Do NOT resend identical tool parameters. Re-anchor with a lightweight read/list action, then adjust arguments.".to_string()),
                ),
            ));
        }
        self.recent_tool_calls.push(call_sig);
        if self.recent_tool_calls.len() > 10 {
            self.recent_tool_calls.remove(0);
        }

        self.cost_tracker.record_tool_call();
        debug!(
            "Tool execute start: tool={} args={}",
            tool_call.name, tool_call.arguments
        );
        let start_time = std::time::Instant::now();

        let (p_tx, mut p_rx) = mpsc::unbounded_channel::<yode_tools::tool::ToolProgress>();
        let event_tx_inner = event_tx.clone();
        let tc_id = tool_call.id.clone();
        let tc_name = tool_call.name.clone();
        let progress_counter = Arc::new(AtomicU64::new(0));
        let progress_counter_inner = Arc::clone(&progress_counter);
        let last_progress_message = Arc::new(std::sync::Mutex::new(None::<String>));
        let last_progress_message_inner = Arc::clone(&last_progress_message);
        tokio::spawn(async move {
            while let Some(progress) = p_rx.recv().await {
                progress_counter_inner.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut slot) = last_progress_message_inner.lock() {
                    *slot = Some(progress.message.clone());
                }
                let _ = event_tx_inner.send(EngineEvent::ToolProgress {
                    id: tc_id.clone(),
                    name: tc_name.clone(),
                    progress,
                });
            }
        });

        let ctx = self.build_tool_context(Some(p_tx)).await;

        let schema = tool.parameters_schema();
        if let Err(msg) = validation::validate_and_coerce(&schema, &mut params) {
            return Ok(Self::immediate_tool_outcome(
                tool_call,
                &started_at,
                ToolResult::error_typed(
                    format!("Parameter validation failed: {}", msg),
                    ToolErrorType::Validation,
                    true,
                    Some(format!("Fix the parameters and retry. Schema: {}", schema)),
                ),
            ));
        }

        let mut result = match tokio::time::timeout(
            std::time::Duration::from_secs(120),
            tool.execute(params, &ctx),
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => {
                error!("Tool {} failed: {}", tool_call.name, e);
                ToolResult::error(format!("Tool execution failed: {}", e))
            }
            Err(_) => ToolResult::error_typed(
                format!("Tool execution timed out after 120s: {}", tool_call.name),
                ToolErrorType::Timeout,
                true,
                Some("Narrow the command scope or run a lighter probe first.".to_string()),
            ),
        };
        let elapsed = start_time.elapsed();
        debug!(
            tool = %tool_call.name,
            elapsed_ms = elapsed.as_millis() as u64,
            "Tool execution completed"
        );

        if result.is_error {
            let auto_hint = match result.error_type {
                Some(ToolErrorType::NotFound) => {
                    Some("Try using `glob` to find the correct path, or `grep` to search for the symbol by name.".to_string())
                }
                Some(ToolErrorType::Validation) => {
                    Some(format!(
                        "Re-check parameter types and required fields. Schema: {}",
                        tool.parameters_schema()
                    ))
                }
                Some(ToolErrorType::Timeout) => {
                    Some("Reduce the scope of the operation (smaller file range, fewer results) and retry.".to_string())
                }
                Some(ToolErrorType::Permission) => {
                    Some("This operation requires user confirmation. The user denied it — try an alternative approach.".to_string())
                }
                _ => None,
            };

            if let Some(ref suggestion) = result.suggestion {
                result
                    .content
                    .push_str(&format!("\n\nSuggestion: {}", suggestion));
            } else if let Some(hint) = auto_hint {
                result
                    .content
                    .push_str(&format!("\n\nSuggestion: {}", hint));
            }
        }

        let progress_updates = progress_counter.load(Ordering::Relaxed) as u32;
        let last_progress_message = last_progress_message
            .lock()
            .ok()
            .and_then(|slot| slot.clone());
        self.record_tool_progress_summary(
            &tool_call.name,
            progress_updates,
            last_progress_message.clone(),
        );

        Ok(ToolExecutionOutcome {
            tool_call: tool_call.clone(),
            result,
            started_at,
            duration_ms: elapsed.as_millis() as u64,
            progress_updates,
            last_progress_message,
            parallel_batch: None,
        })
    }
}
