use serde_json::{json, Value};
use yode_core::engine::EngineEvent;

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeEventParts {
    pub kind: &'static str,
    pub payload: Value,
    pub pending_confirmation: Option<PendingConfirmationParts>,
}

pub type DesktopEventParts = RuntimeEventParts;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingConfirmationParts {
    pub tool_name: String,
    pub command: Option<String>,
}

pub fn engine_event_to_runtime_parts(event: EngineEvent) -> RuntimeEventParts {
    match event {
        EngineEvent::Thinking => RuntimeEventParts {
            kind: "turn_started",
            payload: json!({ "title": "思考中...", "body": "" }),
            pending_confirmation: None,
        },
        EngineEvent::UsageUpdate(usage) => RuntimeEventParts {
            kind: "usage_update",
            payload: json!({
                "title": "用量更新",
                "body": format!("输入 {}，输出 {}", usage.prompt_tokens, usage.completion_tokens),
                "inputTokens": usage.prompt_tokens,
                "outputTokens": usage.completion_tokens,
                "status": "running"
            }),
            pending_confirmation: None,
        },
        EngineEvent::TextDelta(text) => RuntimeEventParts {
            kind: "assistant_text_delta",
            payload: json!({ "body": text }),
            pending_confirmation: None,
        },
        EngineEvent::ActionNarrative(text) => RuntimeEventParts {
            kind: "action_narrative",
            payload: json!({ "body": text, "status": "success" }),
            pending_confirmation: None,
        },
        EngineEvent::TextComplete(text) => RuntimeEventParts {
            kind: "assistant_text_complete",
            payload: json!({ "body": text, "status": "completed" }),
            pending_confirmation: None,
        },
        EngineEvent::ReasoningDelta(reasoning) => RuntimeEventParts {
            kind: "assistant_reasoning_delta",
            payload: json!({ "reasoning": reasoning }),
            pending_confirmation: None,
        },
        EngineEvent::ReasoningComplete(reasoning) => RuntimeEventParts {
            kind: "assistant_reasoning_complete",
            payload: json!({ "reasoning": reasoning, "status": "completed" }),
            pending_confirmation: None,
        },
        EngineEvent::ToolCallStart {
            id,
            name,
            arguments,
        } => RuntimeEventParts {
            kind: "tool_started",
            payload: json!({
                "id": id,
                "tool": name,
                "title": format!("调用工具: {}", name),
                "body": arguments,
                "status": "running"
            }),
            pending_confirmation: None,
        },
        EngineEvent::ToolConfirmRequired {
            id,
            name,
            arguments,
        } => RuntimeEventParts {
            kind: "tool_confirm_required",
            payload: json!({
                "id": id,
                "tool": name,
                "title": format!("请求执行工具: {}", name),
                "body": arguments,
                "meta": "危险操作需要授权"
            }),
            pending_confirmation: Some(PendingConfirmationParts {
                command: extract_command_for_permission(&name, &arguments),
                tool_name: name,
            }),
        },
        EngineEvent::ToolProgress { id, name, progress } => RuntimeEventParts {
            kind: "tool_progress",
            payload: json!({
                "id": id,
                "tool": name,
                "title": format!("工具进度: {}", name),
                "body": progress.message,
                "percent": progress.percent,
                "status": "running"
            }),
            pending_confirmation: None,
        },
        EngineEvent::ToolResult { id, name, result } => {
            let (status, body) = if result.is_error {
                ("blocked", result.content.clone())
            } else {
                ("success", result.content.clone())
            };
            RuntimeEventParts {
                kind: "tool_result",
                payload: json!({
                    "id": id,
                    "tool": name,
                    "title": format!("工具返回: {}", name),
                    "body": body,
                    "status": status,
                    "errorType": result.error_type.map(|kind| format!("{:?}", kind)),
                    "recoverable": result.recoverable,
                    "suggestion": result.suggestion,
                    "metadata": result.metadata
                }),
                pending_confirmation: None,
            }
        }
        EngineEvent::TurnComplete(response) => RuntimeEventParts {
            kind: "turn_completed",
            payload: json!({
                "status": "completed",
                "body": response.message.content.unwrap_or_default(),
                "reasoning": response.message.reasoning.unwrap_or_default(),
                "hasToolCalls": !response.message.tool_calls.is_empty(),
                "toolCallCount": response.message.tool_calls.len(),
                "model": response.model,
                "stopReason": response.stop_reason.map(|reason| format!("{:?}", reason)),
                "inputTokens": response.usage.prompt_tokens,
                "outputTokens": response.usage.completion_tokens,
                "totalTokens": response.usage.total_tokens,
                "contextPercent": 0
            }),
            pending_confirmation: None,
        },
        EngineEvent::Error(err_msg) => RuntimeEventParts {
            kind: "error",
            payload: json!({ "body": err_msg }),
            pending_confirmation: None,
        },
        EngineEvent::Retrying {
            error_message,
            attempt,
            max_attempts,
            delay_secs,
        } => RuntimeEventParts {
            kind: "retrying",
            payload: json!({
                "title": "正在重试",
                "body": error_message,
                "attempt": attempt,
                "maxAttempts": max_attempts,
                "delaySecs": delay_secs,
                "status": "running"
            }),
            pending_confirmation: None,
        },
        EngineEvent::AskUser { id, question } => RuntimeEventParts {
            kind: "ask_user",
            payload: json!({
                "id": id,
                "title": "需要用户输入",
                "body": question,
                "tool": "ask_user",
                "meta": "等待用户回答"
            }),
            pending_confirmation: None,
        },
        EngineEvent::Done => RuntimeEventParts {
            kind: "done",
            payload: json!({
                "title": "完成",
                "body": "本轮已完成。",
                "status": "completed"
            }),
            pending_confirmation: None,
        },
        EngineEvent::SubAgentStart { description } => RuntimeEventParts {
            kind: "subagent_started",
            payload: json!({
                "title": "子代理启动",
                "body": description,
                "tool": "agent",
                "status": "running"
            }),
            pending_confirmation: None,
        },
        EngineEvent::SubAgentComplete { result } => RuntimeEventParts {
            kind: "subagent_completed",
            payload: json!({
                "title": "子代理完成",
                "body": result,
                "tool": "agent",
                "status": "success"
            }),
            pending_confirmation: None,
        },
        EngineEvent::PlanModeEntered => RuntimeEventParts {
            kind: "plan_mode_entered",
            payload: json!({ "title": "计划模式", "body": "已进入计划模式。" }),
            pending_confirmation: None,
        },
        EngineEvent::PlanApprovalRequired { plan_content } => RuntimeEventParts {
            kind: "plan_approval_required",
            payload: json!({
                "title": "计划需要确认",
                "body": plan_content,
                "tool": "plan",
                "meta": "等待确认"
            }),
            pending_confirmation: None,
        },
        EngineEvent::PlanModeExited => RuntimeEventParts {
            kind: "plan_mode_exited",
            payload: json!({ "title": "计划模式", "body": "已退出计划模式。" }),
            pending_confirmation: None,
        },
        EngineEvent::ContextCompactionStarted { mode } => RuntimeEventParts {
            kind: "context_compaction_started",
            payload: json!({
                "title": "上下文压缩开始",
                "body": mode,
                "status": "running"
            }),
            pending_confirmation: None,
        },
        EngineEvent::ContextCompressed {
            mode,
            removed,
            tool_results_truncated,
            summary,
            session_memory_path,
            transcript_path,
        } => RuntimeEventParts {
            kind: "context_compressed",
            payload: json!({
                "title": "上下文已压缩",
                "body": summary.unwrap_or_else(|| format!("模式 {}，移除 {} 条，截断 {} 个工具结果。", mode, removed, tool_results_truncated)),
                "mode": mode,
                "removed": removed,
                "toolResultsTruncated": tool_results_truncated,
                "sessionMemoryPath": session_memory_path,
                "transcriptPath": transcript_path
            }),
            pending_confirmation: None,
        },
        EngineEvent::CostUpdate {
            estimated_cost,
            input_tokens,
            output_tokens,
            cache_write_tokens,
            cache_read_tokens,
        } => RuntimeEventParts {
            kind: "cost_update",
            payload: json!({
                "title": "成本更新",
                "body": format!("${:.4}，输入 {}，输出 {}", estimated_cost, input_tokens, output_tokens),
                "estimatedCost": estimated_cost,
                "inputTokens": input_tokens,
                "outputTokens": output_tokens,
                "cacheWriteTokens": cache_write_tokens,
                "cacheReadTokens": cache_read_tokens
            }),
            pending_confirmation: None,
        },
        EngineEvent::BudgetExceeded { cost, limit } => RuntimeEventParts {
            kind: "budget_exceeded",
            payload: json!({
                "title": "预算已超出",
                "body": format!("当前成本 ${:.4}，限制 ${:.4}", cost, limit),
                "status": "blocked"
            }),
            pending_confirmation: None,
        },
        EngineEvent::SuggestionReady { suggestion } => RuntimeEventParts {
            kind: "suggestion_ready",
            payload: json!({ "title": "建议", "body": suggestion }),
            pending_confirmation: None,
        },
        EngineEvent::SessionMemoryUpdated {
            path,
            generated_summary,
        } => RuntimeEventParts {
            kind: "session_memory_updated",
            payload: json!({
                "title": "会话记忆已更新",
                "body": path,
                "generatedSummary": generated_summary
            }),
            pending_confirmation: None,
        },
        EngineEvent::UpdateAvailable(version) => RuntimeEventParts {
            kind: "update_available",
            payload: json!({ "title": "发现更新", "body": version }),
            pending_confirmation: None,
        },
        EngineEvent::UpdateDownloading => RuntimeEventParts {
            kind: "update_downloading",
            payload: json!({ "title": "正在下载更新", "body": "" }),
            pending_confirmation: None,
        },
        EngineEvent::UpdateDownloaded(version) => RuntimeEventParts {
            kind: "update_downloaded",
            payload: json!({ "title": "更新已下载", "body": version }),
            pending_confirmation: None,
        },
    }
}

pub fn engine_event_to_desktop_parts(event: EngineEvent) -> DesktopEventParts {
    engine_event_to_runtime_parts(event)
}

fn extract_command_for_permission(tool_name: &str, arguments: &str) -> Option<String> {
    let lower = tool_name.to_ascii_lowercase();
    if !matches!(
        lower.as_str(),
        "bash" | "shell" | "exec_command" | "powershell"
    ) {
        return None;
    }
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|value| {
            value
                .get("command")
                .or_else(|| value.get("cmd"))
                .or_else(|| value.get("script"))
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .or_else(|| Some(arguments.to_string()))
}

#[cfg(test)]
mod tests {
    use yode_core::engine::EngineEvent;
    use yode_llm::types::{ChatResponse, Message, Usage};
    use yode_tools::tool::ToolResult;

    use super::{engine_event_to_desktop_parts, engine_event_to_runtime_parts, RuntimeEventParts};

    #[test]
    fn maps_tool_confirm_and_extracts_shell_command() {
        let mapped = engine_event_to_desktop_parts(EngineEvent::ToolConfirmRequired {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: r#"{"command":"cargo test"}"#.to_string(),
        });

        assert_eq!(mapped.kind, "tool_confirm_required");
        assert_eq!(mapped.payload["tool"], "bash");
        let pending = mapped.pending_confirmation.unwrap();
        assert_eq!(pending.tool_name, "bash");
        assert_eq!(pending.command.as_deref(), Some("cargo test"));
    }

    #[test]
    fn maps_tool_result_status_and_metadata() {
        let mapped = engine_event_to_desktop_parts(EngineEvent::ToolResult {
            id: "call-2".to_string(),
            name: "read_file".to_string(),
            result: ToolResult::success("ok".to_string()),
        });

        assert_eq!(mapped.kind, "tool_result");
        assert_eq!(mapped.payload["status"], "success");
        assert_eq!(mapped.payload["body"], "ok");
        assert!(mapped.pending_confirmation.is_none());
    }

    #[test]
    fn maps_turn_complete_usage() {
        let mapped: RuntimeEventParts =
            engine_event_to_runtime_parts(EngineEvent::TurnComplete(ChatResponse {
                message: Message::assistant("done"),
                usage: Usage {
                    prompt_tokens: 10,
                    completion_tokens: 4,
                    total_tokens: 14,
                    ..Usage::default()
                },
                model: "mock-model".to_string(),
                stop_reason: None,
            }));

        assert_eq!(mapped.kind, "turn_completed");
        assert_eq!(mapped.payload["body"], "done");
        assert_eq!(mapped.payload["inputTokens"], 10);
        assert_eq!(mapped.payload["outputTokens"], 4);
        assert_eq!(mapped.payload["totalTokens"], 14);
    }
}
