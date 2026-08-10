import type { UserQuery } from "./askUser";

/** Turn 状态的明确有限集合：与后端 TurnState 枚举一一对应。 */
export const RUN_STATUSES = [
  "starting",
  "running",
  "waiting_approval",
  "waiting_user",
  "cancelling",
  "completed",
  "cancelled",
  "failed",
  "interrupted"
] as const;

export type RunStatus = (typeof RUN_STATUSES)[number];

export const TERMINAL_RUN_STATUSES: ReadonlySet<RunStatus> = new Set([
  "completed",
  "cancelled",
  "failed",
  "interrupted"
]);

export function isRunStatus(value: unknown): value is RunStatus {
  return typeof value === "string" && (RUN_STATUSES as readonly string[]).includes(value);
}

/** 事件 kind 的有限集合（与后端 DesktopEventKind 一致）。 */
export const DESKTOP_EVENT_KINDS = [
  "turn_started",
  "usage_update",
  "assistant_text_delta",
  "action_narrative",
  "assistant_text_complete",
  "assistant_reasoning_delta",
  "assistant_reasoning_complete",
  "tool_started",
  "tool_confirm_required",
  "tool_progress",
  "tool_result",
  "turn_completed",
  "error",
  "retrying",
  "ask_user",
  "done",
  "cancelling",
  "cancelled",
  "subagent_started",
  "subagent_completed",
  "plan_mode_entered",
  "plan_approval_required",
  "plan_mode_exited",
  "context_compaction_started",
  "context_compressed",
  "cost_update",
  "budget_exceeded",
  "suggestion_ready",
  "session_memory_updated",
  "update_available",
  "update_downloading",
  "update_downloaded"
] as const;

export type DesktopEventKind = (typeof DESKTOP_EVENT_KINDS)[number];

export const TERMINAL_TURN_EVENT_KINDS: ReadonlySet<string> = new Set([
  "cancelled",
  "done",
  "turn_completed",
  "error"
]);

/** RunState：后端持久化 turn journal 的桌面投影。status 使用封闭枚举。 */
export type RunState =
  | {
      sessionId: string;
      turnId: string;
      status: "starting" | "running" | "waiting_approval" | "waiting_user" | "cancelling";
      updatedAt: string;
      detail?: string | null;
      startedAt?: string | null;
      endedAt?: string | null;
      lastSeq?: number;
      errorCode?: string | null;
      cancellationRequested?: boolean;
    }
  | {
      sessionId: string;
      turnId: string;
      status: "completed" | "cancelled" | "failed" | "interrupted";
      updatedAt: string;
      detail?: string | null;
      startedAt?: string | null;
      endedAt?: string | null;
      lastSeq?: number;
      errorCode?: string | null;
      cancellationRequested?: boolean;
    };

export type Bootstrap = {
  appVersion: string;
  workspacePath: string;
  /** 后端计算的唯一工作区信任状态（仓库外存储绑定 path+hash+remote）。 */
  workspaceTrusted: boolean;
  provider: string;
  model: string;
  permissionMode: string;
  /** 后端计算后的唯一有效权限模式，前端不得自行推导。 */
  effectivePermissionMode: string;
  sessions: SessionSummary[];
  runs: RunState[];
};

export type DefaultLlm = {
  provider: string;
  model: string;
};

export type ViewMode = "chat" | "settings";

export type SessionSummary = {
  id: string;
  title: string;
  project?: string | null;
  projectRoot?: string | null;
  provider?: string;
  model?: string;
  updatedAt: string;
  active?: boolean;
};

export type TimelineItem =
  | {
      id: string;
      kind: "user" | "assistant" | "reasoning";
      title: string;
      body: string;
      attachments?: ImageAttachment[];
      meta?: string;
      createdAt?: number;
      reasoningStartedAt?: number;
    }
  | {
      id: string;
      kind: "tool";
      title: string;
      body: string;
      status: "running" | "success" | "blocked";
      tool: string;
      callId?: string;
      createdAt?: number;
      meta?: string;
      result?: string;
      metadata?: unknown;
    }

  | {
      id: string;
      kind: "permission";
      title: string;
      body: string;
      tool: string;
      risk: string;
      sessionId?: string;
      turnId?: string;
      createdAt?: number;
    }
  | {
      id: string;
      kind: "boundary";
      title: string;
      body: string;
      createdAt?: number;
    }
  | {
      id: string;
      kind: "process_note";
      title?: string;
      body: string;
      status: "running" | "success";
      createdAt?: number;
    }
  | {
      id: string;
      kind: "error";
      title: string;
      body: string;
      createdAt?: number;
      metadata?: unknown;
    }
  | {
      id: string;
      kind: "activity_group";
      label: string;
      type: "explore" | "search" | "run" | "mixed" | "other";
      status: "running" | "success";
      items: TimelineItem[];
      createdAt?: number;
    }
  | {
      id: string;
      kind: "activity_item";
      type: "edit";
      tool: string;
      title: string;
      body: string;
      status: "running" | "success" | "blocked";
      callId?: string;
      filename?: string;
      diff?: string;
      result?: string;
      metadata?: unknown;
      createdAt?: number;
    }
  | {
      id: string;
      kind: "edit_summary";
      status: "running" | "success" | "blocked";
      items: Array<Extract<TimelineItem, { kind: "activity_item" }>>;
      createdAt?: number;
    }
  | {
      id: string;
      kind: "tool_group";
      label: string;
      icon: string;
      type: "explore" | "search" | "edit" | "run" | "other";
      status: "running" | "success";
      items?: TimelineItem[];
      createdAt?: number;
    };

/**
 * DesktopEventEnvelope：统一事件信封（schemaVersion + sessionId + turnId + seq +
 * timestamp + kind + payload）。payload 按 kind 分化的强类型结构；
 * 未知字段允许扩展，错误类型由 validateDesktopEventEnvelope 拒绝。
 */
export type DesktopEventEnvelope = {
  schemaVersion?: number;
  sessionId: string;
  turnId: string;
  seq: number;
  timestamp: string;
  kind: DesktopEventKind;
  payload: TurnEventPayload;
};

export type DesktopEvent = DesktopEventEnvelope;

/** 事件 payload：按 kind 分化的 tagged union。 */
export type TurnEventPayload =
  | { title?: string; body?: string }
  | { id?: string; tool?: string; title?: string; body?: string; status?: string; meta?: string }
  | {
      body?: string;
      status?: string;
      inputTokens?: number;
      outputTokens?: number;
      totalTokens?: number;
      estimatedCost?: number;
      cacheWriteTokens?: number;
      cacheReadTokens?: number;
      model?: string;
      stopReason?: string;
      hasToolCalls?: boolean;
      toolCallCount?: number;
      reasoning?: string;
      attempt?: number;
      maxAttempts?: number;
      delaySecs?: number;
      percent?: number;
      errorType?: string;
      recoverable?: boolean;
      suggestion?: string;
      metadata?: unknown;
      mode?: string;
      removed?: number;
      toolResultsTruncated?: number;
      sessionMemoryPath?: string;
      transcriptPath?: string;
      generatedSummary?: boolean;
      query?: UserQuery;
    };

/** 后端持久化 turn 事件（重放/恢复用）。payload 已脱敏。 */
export type TurnEventRecord = {
  sessionId: string;
  turnId: string;
  seq: number;
  kind: string;
  timestamp: string;
  payload: Record<string, unknown>;
};

export type SessionMessagesPage = {
  messages: DesktopMessage[];
  hasMore: boolean;
};

export type DesktopMessage = {
  id: number;
  /** 会话内消息顺序（分页游标）；旧版响应可能缺失。 */
  sortOrder?: number;
  role: string;
  content?: string | null;
  reasoning?: string | null;
  toolCallsJson?: string | null;
  toolCallId?: string | null;
  metadata?: unknown;
  images?: Array<{
    base64: string;
    mediaType: string;
  }>;
  createdAt: string;
};

export type TurnAccepted = {
  sessionId: string;
  turnId: string;
  session: SessionSummary;
};

export type ImageAttachment = {
  id: string;
  name: string;
  mediaType: string;
  base64: string;
  dataUrl: string;
  size: number;
  width?: number;
  height?: number;
};

export type PendingUserQuestion = {
  sessionId: string;
  turnId: string;
  title?: string;
  question: string;
  query?: UserQuery;
};

export type UsageSnapshot = {
  estimatedCost?: number;
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
  cacheWriteTokens?: number;
  cacheReadTokens?: number;
};

export const fallbackBootstrap: Bootstrap = {
  appVersion: "",
  workspacePath: "",
  workspaceTrusted: false,
  provider: "",
  model: "",
  permissionMode: "default",
  effectivePermissionMode: "default",
  sessions: [],
  runs: []
};
