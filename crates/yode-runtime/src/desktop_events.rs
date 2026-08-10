use serde::Serialize;
use serde_json::{json, Value};
use yode_core::engine::EngineEvent;

/// 强类型桌面事件 kind：与历史字符串事件命名保持一致（snake_case），
/// 未知 kind 必须安全保留到诊断日志，不得导致前端崩溃。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopEventKind {
    TurnStarted,
    UsageUpdate,
    AssistantTextDelta,
    ActionNarrative,
    AssistantTextComplete,
    AssistantReasoningDelta,
    AssistantReasoningComplete,
    ToolStarted,
    ToolConfirmRequired,
    ToolProgress,
    ToolResult,
    TurnCompleted,
    Error,
    Retrying,
    AskUser,
    Done,
    Cancelling,
    Cancelled,
    SubAgentStarted,
    SubAgentCompleted,
    PlanModeEntered,
    PlanApprovalRequired,
    PlanModeExited,
    ContextCompactionStarted,
    ContextCompressed,
    CostUpdate,
    BudgetExceeded,
    SuggestionReady,
    SessionMemoryUpdated,
    UpdateAvailable,
    UpdateDownloading,
    UpdateDownloaded,
}

impl DesktopEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TurnStarted => "turn_started",
            Self::UsageUpdate => "usage_update",
            Self::AssistantTextDelta => "assistant_text_delta",
            Self::ActionNarrative => "action_narrative",
            Self::AssistantTextComplete => "assistant_text_complete",
            Self::AssistantReasoningDelta => "assistant_reasoning_delta",
            Self::AssistantReasoningComplete => "assistant_reasoning_complete",
            Self::ToolStarted => "tool_started",
            Self::ToolConfirmRequired => "tool_confirm_required",
            Self::ToolProgress => "tool_progress",
            Self::ToolResult => "tool_result",
            Self::TurnCompleted => "turn_completed",
            Self::Error => "error",
            Self::Retrying => "retrying",
            Self::AskUser => "ask_user",
            Self::Done => "done",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::SubAgentStarted => "subagent_started",
            Self::SubAgentCompleted => "subagent_completed",
            Self::PlanModeEntered => "plan_mode_entered",
            Self::PlanApprovalRequired => "plan_approval_required",
            Self::PlanModeExited => "plan_mode_exited",
            Self::ContextCompactionStarted => "context_compaction_started",
            Self::ContextCompressed => "context_compressed",
            Self::CostUpdate => "cost_update",
            Self::BudgetExceeded => "budget_exceeded",
            Self::SuggestionReady => "suggestion_ready",
            Self::SessionMemoryUpdated => "session_memory_updated",
            Self::UpdateAvailable => "update_available",
            Self::UpdateDownloading => "update_downloading",
            Self::UpdateDownloaded => "update_downloaded",
        }
    }

    /// 解析历史字符串 kind；未知值返回 None（由调用方安全保留到诊断）。
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "turn_started" => Self::TurnStarted,
            "usage_update" => Self::UsageUpdate,
            "assistant_text_delta" => Self::AssistantTextDelta,
            "action_narrative" => Self::ActionNarrative,
            "assistant_text_complete" => Self::AssistantTextComplete,
            "assistant_reasoning_delta" => Self::AssistantReasoningDelta,
            "assistant_reasoning_complete" => Self::AssistantReasoningComplete,
            "tool_started" => Self::ToolStarted,
            "tool_confirm_required" => Self::ToolConfirmRequired,
            "tool_progress" => Self::ToolProgress,
            "tool_result" => Self::ToolResult,
            "turn_completed" => Self::TurnCompleted,
            "error" => Self::Error,
            "retrying" => Self::Retrying,
            "ask_user" => Self::AskUser,
            "done" => Self::Done,
            "cancelling" => Self::Cancelling,
            "cancelled" => Self::Cancelled,
            "subagent_started" => Self::SubAgentStarted,
            "subagent_completed" => Self::SubAgentCompleted,
            "plan_mode_entered" => Self::PlanModeEntered,
            "plan_approval_required" => Self::PlanApprovalRequired,
            "plan_mode_exited" => Self::PlanModeExited,
            "context_compaction_started" => Self::ContextCompactionStarted,
            "context_compressed" => Self::ContextCompressed,
            "cost_update" => Self::CostUpdate,
            "budget_exceeded" => Self::BudgetExceeded,
            "suggestion_ready" => Self::SuggestionReady,
            "session_memory_updated" => Self::SessionMemoryUpdated,
            "update_available" => Self::UpdateAvailable,
            "update_downloading" => Self::UpdateDownloading,
            "update_downloaded" => Self::UpdateDownloaded,
            _ => return None,
        })
    }

    /// 是否属于 turn 终态事件（前端可据此停止取消轮询、解锁 UI）。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::TurnCompleted | Self::Error | Self::Done | Self::Cancelled
        )
    }
}

/// 事件 kind 到 run 状态的映射：仅供运行时在事件写入临界区内同步持久化状态。
/// 非状态类事件返回 None。
pub fn run_status_for_event_kind(kind: DesktopEventKind) -> Option<&'static str> {
    match kind {
        DesktopEventKind::ToolConfirmRequired | DesktopEventKind::PlanApprovalRequired => {
            Some("waiting_approval")
        }
        DesktopEventKind::Error => Some("failed"),
        DesktopEventKind::TurnCompleted | DesktopEventKind::Done => Some("completed"),
        DesktopEventKind::Cancelled => Some("cancelled"),
        DesktopEventKind::Cancelling => Some("cancelling"),
        DesktopEventKind::TurnStarted
        | DesktopEventKind::ToolStarted
        | DesktopEventKind::AssistantTextDelta
        | DesktopEventKind::AssistantReasoningDelta
        | DesktopEventKind::AskUser => Some("running"),
        _ => None,
    }
}

/// 各事件的强类型 payload。字段与历史 JSON 完全一致（向后兼容），
/// 新增稳定字段逐步补充。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum DesktopEventPayload {
    TurnStarted(TurnStartedPayload),
    UsageUpdate(UsageUpdatePayload),
    AssistantTextDelta(TextDeltaPayload),
    ActionNarrative(TextStatusPayload),
    AssistantTextComplete(TextStatusPayload),
    AssistantReasoningDelta(ReasoningDeltaPayload),
    AssistantReasoningComplete(ReasoningCompletePayload),
    ToolStarted(ToolStartedPayload),
    ToolConfirmRequired(ToolConfirmRequiredPayload),
    ToolProgress(ToolProgressPayload),
    ToolResult(ToolResultPayload),
    TurnCompleted(TurnCompletedPayload),
    Error(ErrorPayload),
    Retrying(RetryingPayload),
    AskUser(AskUserPayload),
    Done(DonePayload),
    Cancelling(CancellingPayload),
    Cancelled(CancelledPayload),
    SubAgentStarted(SubAgentPayload),
    SubAgentCompleted(SubAgentPayload),
    PlanModeEntered(PlanModePayload),
    PlanApprovalRequired(PlanApprovalPayload),
    PlanModeExited(PlanModePayload),
    ContextCompactionStarted(ContextCompactionStartedPayload),
    ContextCompressed(ContextCompressedPayload),
    CostUpdate(CostUpdatePayload),
    BudgetExceeded(BudgetExceededPayload),
    SuggestionReady(SuggestionReadyPayload),
    SessionMemoryUpdated(SessionMemoryUpdatedPayload),
    UpdateAvailable(UpdateAvailablePayload),
    UpdateDownloading(UpdateDownloadingPayload),
    UpdateDownloaded(UpdateDownloadedPayload),
}

impl DesktopEventPayload {
    pub fn as_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({ "serialize_error": true }))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartedPayload {
    pub title: &'static str,
    pub body: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageUpdatePayload {
    pub title: &'static str,
    pub body: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDeltaPayload {
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextStatusPayload {
    pub body: String,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningDeltaPayload {
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningCompletePayload {
    pub reasoning: String,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStartedPayload {
    pub id: String,
    pub tool: String,
    pub title: String,
    pub body: String,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfirmRequiredPayload {
    pub id: String,
    pub tool: String,
    pub title: String,
    pub body: String,
    pub meta: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgressPayload {
    pub id: String,
    pub tool: String,
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultPayload {
    pub id: String,
    pub tool: String,
    pub title: String,
    pub body: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    pub recoverable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnCompletedPayload {
    pub status: &'static str,
    pub body: String,
    pub reasoning: String,
    pub has_tool_calls: bool,
    pub tool_call_count: usize,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub context_percent: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryingPayload {
    pub title: &'static str,
    pub body: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub delay_secs: u64,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskUserPayload {
    pub id: String,
    pub title: String,
    pub body: String,
    pub tool: &'static str,
    pub meta: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DonePayload {
    pub title: &'static str,
    pub body: &'static str,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancellingPayload {
    pub title: &'static str,
    pub body: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelledPayload {
    pub title: &'static str,
    pub body: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubAgentPayload {
    pub title: &'static str,
    pub body: String,
    pub tool: &'static str,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanModePayload {
    pub title: &'static str,
    pub body: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanApprovalPayload {
    pub title: &'static str,
    pub body: String,
    pub tool: &'static str,
    pub meta: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCompactionStartedPayload {
    pub title: &'static str,
    pub body: String,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCompressedPayload {
    pub title: &'static str,
    pub body: String,
    pub mode: String,
    pub removed: usize,
    pub tool_results_truncated: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_memory_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostUpdatePayload {
    pub title: &'static str,
    pub body: String,
    pub estimated_cost: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_tokens: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetExceededPayload {
    pub title: &'static str,
    pub body: String,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionReadyPayload {
    pub title: &'static str,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMemoryUpdatedPayload {
    pub title: &'static str,
    pub body: String,
    pub generated_summary: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAvailablePayload {
    pub title: &'static str,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDownloadingPayload {
    pub title: &'static str,
    pub body: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDownloadedPayload {
    pub title: &'static str,
    pub body: String,
}

/// EngineEvent → 桌面事件的唯一适配产物（强类型）。
#[derive(Debug, Clone)]
pub struct RuntimeEventParts {
    pub kind: DesktopEventKind,
    pub payload: DesktopEventPayload,
    pub pending_confirmation: Option<PendingConfirmationParts>,
}

pub type DesktopEventParts = RuntimeEventParts;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingConfirmationParts {
    pub tool_name: String,
    pub command: Option<String>,
}

/// 统一事件信封：schemaVersion 稳定，老字段继续输出，新字段只增不改。
/// 事件信封的唯一构造入口：kind 必须来自 `DesktopEventKind`（不允许裸字符串）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEventEnvelope {
    pub schema_version: u32,
    pub session_id: String,
    pub turn_id: String,
    pub seq: u64,
    pub timestamp: String,
    pub kind: DesktopEventKind,
    pub payload: Value,
}

impl DesktopEventEnvelope {
    pub fn new(
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
        seq: u64,
        timestamp: String,
        kind: DesktopEventKind,
        payload: Value,
    ) -> Self {
        Self {
            schema_version: 1,
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            seq,
            timestamp,
            kind,
            payload,
        }
    }

    /// 序列化为前端可见的完整 JSON（含 payload 强类型字段）。
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({ "serialize_error": true }))
    }
}

pub fn engine_event_to_runtime_parts(event: EngineEvent) -> RuntimeEventParts {
    match event {
        EngineEvent::Thinking => RuntimeEventParts {
            kind: DesktopEventKind::TurnStarted,
            payload: DesktopEventPayload::TurnStarted(TurnStartedPayload {
                title: "思考中...",
                body: "",
            }),
            pending_confirmation: None,
        },
        EngineEvent::UsageUpdate(usage) => RuntimeEventParts {
            kind: DesktopEventKind::UsageUpdate,
            payload: DesktopEventPayload::UsageUpdate(UsageUpdatePayload {
                title: "用量更新",
                body: format!(
                    "输入 {}，输出 {}",
                    usage.prompt_tokens, usage.completion_tokens
                ),
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
                status: "running",
            }),
            pending_confirmation: None,
        },
        EngineEvent::TextDelta(text) => RuntimeEventParts {
            kind: DesktopEventKind::AssistantTextDelta,
            payload: DesktopEventPayload::AssistantTextDelta(TextDeltaPayload { body: text }),
            pending_confirmation: None,
        },
        EngineEvent::ActionNarrative(text) => RuntimeEventParts {
            kind: DesktopEventKind::ActionNarrative,
            payload: DesktopEventPayload::ActionNarrative(TextStatusPayload {
                body: text,
                status: "success",
            }),
            pending_confirmation: None,
        },
        EngineEvent::TextComplete(text) => RuntimeEventParts {
            kind: DesktopEventKind::AssistantTextComplete,
            payload: DesktopEventPayload::AssistantTextComplete(TextStatusPayload {
                body: text,
                status: "completed",
            }),
            pending_confirmation: None,
        },
        EngineEvent::ReasoningDelta(reasoning) => RuntimeEventParts {
            kind: DesktopEventKind::AssistantReasoningDelta,
            payload: DesktopEventPayload::AssistantReasoningDelta(ReasoningDeltaPayload {
                reasoning,
            }),
            pending_confirmation: None,
        },
        EngineEvent::ReasoningComplete(reasoning) => RuntimeEventParts {
            kind: DesktopEventKind::AssistantReasoningComplete,
            payload: DesktopEventPayload::AssistantReasoningComplete(ReasoningCompletePayload {
                reasoning,
                status: "completed",
            }),
            pending_confirmation: None,
        },
        EngineEvent::ToolCallStart {
            id,
            name,
            arguments,
        } => RuntimeEventParts {
            kind: DesktopEventKind::ToolStarted,
            payload: DesktopEventPayload::ToolStarted(ToolStartedPayload {
                id,
                tool: name.clone(),
                title: format!("调用工具: {}", name),
                body: arguments,
                status: "running",
            }),
            pending_confirmation: None,
        },
        EngineEvent::ToolConfirmRequired {
            id,
            name,
            arguments,
        } => RuntimeEventParts {
            kind: DesktopEventKind::ToolConfirmRequired,
            payload: DesktopEventPayload::ToolConfirmRequired(ToolConfirmRequiredPayload {
                id,
                tool: name.clone(),
                title: format!("请求执行工具: {}", name),
                body: arguments.clone(),
                meta: "危险操作需要授权",
            }),
            pending_confirmation: Some(PendingConfirmationParts {
                command: extract_command_for_permission(&name, &arguments),
                tool_name: name,
            }),
        },
        EngineEvent::ToolProgress { id, name, progress } => RuntimeEventParts {
            kind: DesktopEventKind::ToolProgress,
            payload: DesktopEventPayload::ToolProgress(ToolProgressPayload {
                id,
                tool: name.clone(),
                title: format!("工具进度: {}", name),
                body: progress.message,
                percent: progress.percent,
                status: "running",
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
                kind: DesktopEventKind::ToolResult,
                payload: DesktopEventPayload::ToolResult(ToolResultPayload {
                    id,
                    tool: name.clone(),
                    title: format!("工具返回: {}", name),
                    body,
                    status,
                    error_type: result.error_type.map(|kind| format!("{:?}", kind)),
                    recoverable: result.recoverable,
                    suggestion: result.suggestion,
                    metadata: result.metadata,
                }),
                pending_confirmation: None,
            }
        }
        EngineEvent::TurnComplete(response) => RuntimeEventParts {
            kind: DesktopEventKind::TurnCompleted,
            payload: DesktopEventPayload::TurnCompleted(TurnCompletedPayload {
                status: "completed",
                body: response.message.content.unwrap_or_default(),
                reasoning: response.message.reasoning.unwrap_or_default(),
                has_tool_calls: !response.message.tool_calls.is_empty(),
                tool_call_count: response.message.tool_calls.len(),
                model: response.model,
                stop_reason: response.stop_reason.map(|reason| format!("{:?}", reason)),
                input_tokens: response.usage.prompt_tokens,
                output_tokens: response.usage.completion_tokens,
                total_tokens: response.usage.total_tokens,
                context_percent: 0,
            }),
            pending_confirmation: None,
        },
        EngineEvent::Error(err_msg) => RuntimeEventParts {
            kind: DesktopEventKind::Error,
            payload: DesktopEventPayload::Error(ErrorPayload { body: err_msg }),
            pending_confirmation: None,
        },
        EngineEvent::Retrying {
            error_message,
            attempt,
            max_attempts,
            delay_secs,
        } => RuntimeEventParts {
            kind: DesktopEventKind::Retrying,
            payload: DesktopEventPayload::Retrying(RetryingPayload {
                title: "正在重试",
                body: error_message,
                attempt,
                max_attempts,
                delay_secs,
                status: "running",
            }),
            pending_confirmation: None,
        },
        EngineEvent::AskUser { id, question } => RuntimeEventParts {
            kind: DesktopEventKind::AskUser,
            payload: DesktopEventPayload::AskUser(AskUserPayload {
                id,
                title: "需要用户输入".to_string(),
                body: question,
                tool: "ask_user",
                meta: "等待用户回答",
                query: None,
            }),
            pending_confirmation: None,
        },
        EngineEvent::Done => RuntimeEventParts {
            kind: DesktopEventKind::Done,
            payload: DesktopEventPayload::Done(DonePayload {
                title: "完成",
                body: "本轮已完成。",
                status: "completed",
            }),
            pending_confirmation: None,
        },
        EngineEvent::SubAgentStart { description } => RuntimeEventParts {
            kind: DesktopEventKind::SubAgentStarted,
            payload: DesktopEventPayload::SubAgentStarted(SubAgentPayload {
                title: "子代理启动",
                body: description,
                tool: "agent",
                status: "running",
            }),
            pending_confirmation: None,
        },
        EngineEvent::SubAgentComplete { result } => RuntimeEventParts {
            kind: DesktopEventKind::SubAgentCompleted,
            payload: DesktopEventPayload::SubAgentCompleted(SubAgentPayload {
                title: "子代理完成",
                body: result,
                tool: "agent",
                status: "success",
            }),
            pending_confirmation: None,
        },
        EngineEvent::PlanModeEntered => RuntimeEventParts {
            kind: DesktopEventKind::PlanModeEntered,
            payload: DesktopEventPayload::PlanModeEntered(PlanModePayload {
                title: "计划模式",
                body: "已进入计划模式。",
            }),
            pending_confirmation: None,
        },
        EngineEvent::PlanApprovalRequired { plan_content } => RuntimeEventParts {
            kind: DesktopEventKind::PlanApprovalRequired,
            payload: DesktopEventPayload::PlanApprovalRequired(PlanApprovalPayload {
                title: "计划需要确认",
                body: plan_content,
                tool: "plan",
                meta: "等待确认",
            }),
            pending_confirmation: None,
        },
        EngineEvent::PlanModeExited => RuntimeEventParts {
            kind: DesktopEventKind::PlanModeExited,
            payload: DesktopEventPayload::PlanModeExited(PlanModePayload {
                title: "计划模式",
                body: "已退出计划模式。",
            }),
            pending_confirmation: None,
        },
        EngineEvent::ContextCompactionStarted { mode } => RuntimeEventParts {
            kind: DesktopEventKind::ContextCompactionStarted,
            payload: DesktopEventPayload::ContextCompactionStarted(
                ContextCompactionStartedPayload {
                    title: "上下文压缩开始",
                    body: mode,
                    status: "running",
                },
            ),
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
            kind: DesktopEventKind::ContextCompressed,
            payload: DesktopEventPayload::ContextCompressed(ContextCompressedPayload {
                title: "上下文已压缩",
                body: summary.unwrap_or_else(|| {
                    format!(
                        "模式 {}，移除 {} 条，截断 {} 个工具结果。",
                        mode, removed, tool_results_truncated
                    )
                }),
                mode,
                removed,
                tool_results_truncated,
                session_memory_path,
                transcript_path,
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
            kind: DesktopEventKind::CostUpdate,
            payload: DesktopEventPayload::CostUpdate(CostUpdatePayload {
                title: "成本更新",
                body: format!(
                    "${:.4}，输入 {}，输出 {}",
                    estimated_cost, input_tokens, output_tokens
                ),
                estimated_cost,
                input_tokens,
                output_tokens,
                cache_write_tokens,
                cache_read_tokens,
            }),
            pending_confirmation: None,
        },
        EngineEvent::BudgetExceeded { cost, limit } => RuntimeEventParts {
            kind: DesktopEventKind::BudgetExceeded,
            payload: DesktopEventPayload::BudgetExceeded(BudgetExceededPayload {
                title: "预算已超出",
                body: format!("当前成本 ${:.4}，限制 ${:.4}", cost, limit),
                status: "blocked",
            }),
            pending_confirmation: None,
        },
        EngineEvent::SuggestionReady { suggestion } => RuntimeEventParts {
            kind: DesktopEventKind::SuggestionReady,
            payload: DesktopEventPayload::SuggestionReady(SuggestionReadyPayload {
                title: "建议",
                body: suggestion,
            }),
            pending_confirmation: None,
        },
        EngineEvent::SessionMemoryUpdated {
            path,
            generated_summary,
        } => RuntimeEventParts {
            kind: DesktopEventKind::SessionMemoryUpdated,
            payload: DesktopEventPayload::SessionMemoryUpdated(SessionMemoryUpdatedPayload {
                title: "会话记忆已更新",
                body: path,
                generated_summary,
            }),
            pending_confirmation: None,
        },
        EngineEvent::UpdateAvailable(version) => RuntimeEventParts {
            kind: DesktopEventKind::UpdateAvailable,
            payload: DesktopEventPayload::UpdateAvailable(UpdateAvailablePayload {
                title: "发现更新",
                body: version,
            }),
            pending_confirmation: None,
        },
        EngineEvent::UpdateDownloading => RuntimeEventParts {
            kind: DesktopEventKind::UpdateDownloading,
            payload: DesktopEventPayload::UpdateDownloading(UpdateDownloadingPayload {
                title: "正在下载更新",
                body: "",
            }),
            pending_confirmation: None,
        },
        EngineEvent::UpdateDownloaded(version) => RuntimeEventParts {
            kind: DesktopEventKind::UpdateDownloaded,
            payload: DesktopEventPayload::UpdateDownloaded(UpdateDownloadedPayload {
                title: "更新已下载",
                body: version,
            }),
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

    use super::{engine_event_to_desktop_parts, engine_event_to_runtime_parts, DesktopEventKind};

    #[test]
    fn maps_tool_confirm_and_extracts_shell_command() {
        let mapped = engine_event_to_desktop_parts(EngineEvent::ToolConfirmRequired {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: r#"{"command":"cargo test"}"#.to_string(),
        });

        assert_eq!(mapped.kind, DesktopEventKind::ToolConfirmRequired);
        assert_eq!(mapped.kind.as_str(), "tool_confirm_required");
        assert_eq!(mapped.payload.as_value()["tool"], "bash");
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

        assert_eq!(mapped.kind, DesktopEventKind::ToolResult);
        assert_eq!(mapped.payload.as_value()["status"], "success");
        assert_eq!(mapped.payload.as_value()["body"], "ok");
        assert!(mapped.pending_confirmation.is_none());
    }

    #[test]
    fn maps_turn_complete_usage() {
        let mapped: super::RuntimeEventParts =
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

        assert_eq!(mapped.kind, DesktopEventKind::TurnCompleted);
        let value = mapped.payload.as_value();
        assert_eq!(value["body"], "done");
        assert_eq!(value["inputTokens"], 10);
        assert_eq!(value["outputTokens"], 4);
        assert_eq!(value["totalTokens"], 14);
    }

    #[test]
    fn kind_parses_known_and_rejects_unknown() {
        assert_eq!(
            DesktopEventKind::parse("tool_confirm_required"),
            Some(DesktopEventKind::ToolConfirmRequired)
        );
        assert_eq!(DesktopEventKind::parse("nope"), None);
    }

    #[test]
    fn run_status_mapping_covers_state_events() {
        use super::run_status_for_event_kind;
        assert_eq!(
            run_status_for_event_kind(DesktopEventKind::ToolConfirmRequired),
            Some("waiting_approval")
        );
        assert_eq!(
            run_status_for_event_kind(DesktopEventKind::Error),
            Some("failed")
        );
        assert_eq!(
            run_status_for_event_kind(DesktopEventKind::TurnCompleted),
            Some("completed")
        );
        assert_eq!(
            run_status_for_event_kind(DesktopEventKind::Cancelled),
            Some("cancelled")
        );
        assert_eq!(
            run_status_for_event_kind(DesktopEventKind::CostUpdate),
            None
        );
    }

    #[test]
    fn all_kinds_serialize_with_expected_strings() {
        for kind in [
            DesktopEventKind::TurnStarted,
            DesktopEventKind::UsageUpdate,
            DesktopEventKind::AssistantTextDelta,
            DesktopEventKind::ActionNarrative,
            DesktopEventKind::AssistantTextComplete,
            DesktopEventKind::AssistantReasoningDelta,
            DesktopEventKind::AssistantReasoningComplete,
            DesktopEventKind::ToolStarted,
            DesktopEventKind::ToolConfirmRequired,
            DesktopEventKind::ToolProgress,
            DesktopEventKind::ToolResult,
            DesktopEventKind::TurnCompleted,
            DesktopEventKind::Error,
            DesktopEventKind::Retrying,
            DesktopEventKind::AskUser,
            DesktopEventKind::Done,
            DesktopEventKind::Cancelling,
            DesktopEventKind::Cancelled,
            DesktopEventKind::SubAgentStarted,
            DesktopEventKind::SubAgentCompleted,
            DesktopEventKind::PlanModeEntered,
            DesktopEventKind::PlanApprovalRequired,
            DesktopEventKind::PlanModeExited,
            DesktopEventKind::ContextCompactionStarted,
            DesktopEventKind::ContextCompressed,
            DesktopEventKind::CostUpdate,
            DesktopEventKind::BudgetExceeded,
            DesktopEventKind::SuggestionReady,
            DesktopEventKind::SessionMemoryUpdated,
            DesktopEventKind::UpdateAvailable,
            DesktopEventKind::UpdateDownloading,
            DesktopEventKind::UpdateDownloaded,
        ] {
            assert_eq!(DesktopEventKind::parse(kind.as_str()), Some(kind));
        }
    }
}
