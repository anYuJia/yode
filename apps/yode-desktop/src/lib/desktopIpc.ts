import { invoke } from "@tauri-apps/api/core";

import type {
  Bootstrap,
  DesktopMessage,
  RunState,
  SessionMessagesPage,
  SessionSummary,
  TurnAccepted,
  TurnEventRecord
} from "./desktopTypes";

/**
 * 所有 IPC invoke 的请求/响应类型集中定义。
 * 组件内不得重复手写 invoke 参数或响应类型。
 */

// ─── 请求参数类型 ──────────────────────────────────────────────────────────

export type SendMessageRequestPayload = {
  sessionId?: string;
  content: string;
  images: Array<{ base64: string; mediaType: string; name?: string }>;
  projectRoot?: string;
  standalone?: boolean;
  title?: string;
  provider?: string;
  model?: string;
};

export type PermissionModeSetRequest = {
  mode: string;
  bypassConfirmed?: boolean;
  scope?: string | null;
};

// ─── 响应类型（与后端 protocol.rs 对应）─────────────────────────────────────

export type PermissionModeState = {
  effectivePermissionMode: string;
  scope: string;
  persisted: boolean;
  bypassActive: boolean;
};

export type RuntimeState = {
  activeSessionId?: string | null;
  status: string;
  permissionMode: string;
  effectivePermissionMode: string;
  contextPercent: number;
  toolCalls: string;
};

export type SessionExportResult = {
  path: string;
  messageCount: number;
};

export type SessionCompactResult = {
  beforeCount: number;
  afterCount: number;
  removedCount: number;
  summary: string;
};

export type DesktopProvider = {
  id: string;
  name: string;
  format: string;
  enabled: boolean;
  apiKey: string;
  hasApiKey: boolean;
  baseUrl: string;
  models: string[];
  gradient?: string | null;
};

export type DefaultLlm = {
  provider: string;
  model: string;
};

export type ImportAiSessionsResult = {
  imported: number;
  skipped: number;
  sessions: SessionSummary[];
};

export type UpdateCheckResult = {
  version: string;
  releaseUrl: string;
  publishedAt: string;
};

// ─── 命令包装 ──────────────────────────────────────────────────────────────

export function appGetBootstrap(): Promise<Bootstrap> {
  return invoke<Bootstrap>("app_get_bootstrap");
}

export function runtimeStateGet(): Promise<RuntimeState> {
  return invoke<RuntimeState>("runtime_state_get");
}

export function runsList(): Promise<RunState[]> {
  return invoke<RunState[]>("runs_list");
}

export function sessionsList(): Promise<SessionSummary[]> {
  return invoke<SessionSummary[]>("sessions_list");
}

export function sessionsMessages(sessionId: string): Promise<DesktopMessage[]> {
  return invoke<DesktopMessage[]>("sessions_messages", { sessionId, session_id: sessionId });
}

export function sessionsMessagesPage(
  sessionId: string,
  before?: number | null,
  limit?: number
): Promise<SessionMessagesPage> {
  return invoke<SessionMessagesPage>("sessions_messages_page", {
    sessionId,
    session_id: sessionId,
    before: before ?? null,
    limit: limit ?? 100
  });
}

export function sessionsClearMessages(sessionId: string): Promise<void> {
  return invoke<void>("sessions_clear_messages", { sessionId });
}

export function sessionsRename(sessionId: string, title: string): Promise<SessionSummary> {
  return invoke<SessionSummary>("sessions_rename", { sessionId, title });
}

export function sessionsDelete(sessionId: string): Promise<void> {
  return invoke<void>("sessions_delete", { sessionId });
}

export function sessionsUpdateLlm(
  sessionId: string,
  provider: string,
  model: string
): Promise<void> {
  return invoke<void>("sessions_update_llm", { sessionId, provider, model });
}

export function sessionsExportMarkdown(sessionId: string): Promise<SessionExportResult> {
  return invoke<SessionExportResult>("sessions_export_markdown", { sessionId });
}

export function sessionsCompactEngine(sessionId: string): Promise<SessionCompactResult> {
  return invoke<SessionCompactResult>("sessions_compact_engine", { sessionId });
}

export function sessionsCompactLocal(sessionId: string): Promise<SessionCompactResult> {
  return invoke<SessionCompactResult>("sessions_compact_local", { sessionId });
}

export function turnSendMessage(request: SendMessageRequestPayload): Promise<TurnAccepted> {
  return invoke<TurnAccepted>("turn_send_message", { request });
}

export function permissionRespond(
  sessionId: string,
  turnId: string,
  allow: boolean,
  alwaysAllow: boolean
): Promise<void> {
  return invoke<void>("permission_respond", {
    sessionId,
    session_id: sessionId,
    turnId,
    turn_id: turnId,
    allow,
    alwaysAllow
  });
}

export function askUserRespond(sessionId: string, turnId: string, answer: string): Promise<void> {
  return invoke<void>("ask_user_respond", {
    sessionId,
    session_id: sessionId,
    turnId,
    turn_id: turnId,
    answer
  });
}

export function turnCancel(sessionId: string, turnId: string): Promise<void> {
  return invoke<void>("turn_cancel", { sessionId, turnId });
}

/** 重放：读取某个 turn 在 sinceSeq 之后的事件（升序）。 */
export function turnEventsSince(
  sessionId: string,
  turnId: string,
  sinceSeq: number,
  limit?: number | null
): Promise<TurnEventRecord[]> {
  return invoke<TurnEventRecord[]>("turn_events_since", {
    sessionId,
    session_id: sessionId,
    turnId,
    turn_id: turnId,
    sinceSeq,
    since_seq: sinceSeq,
    limit: limit ?? null
  });
}

/** RunInspector：读取某个 turn 最近 N 条事件。 */
export function turnRecentEvents(
  sessionId: string,
  turnId: string,
  limit: number
): Promise<TurnEventRecord[]> {
  return invoke<TurnEventRecord[]>("turn_recent_events", {
    sessionId,
    session_id: sessionId,
    turnId,
    turn_id: turnId,
    limit
  });
}

export function permissionModeSet(request: PermissionModeSetRequest): Promise<PermissionModeState> {
  return invoke<PermissionModeState>("permission_mode_set", {
    mode: request.mode,
    bypassConfirmed: request.bypassConfirmed,
    bypass_confirmed: request.bypassConfirmed,
    scope: request.scope
  });
}

export function configGetProviders(): Promise<DesktopProvider[]> {
  return invoke<DesktopProvider[]>("config_get_providers");
}

export function configSaveProviders(providers: DesktopProvider[]): Promise<void> {
  return invoke<void>("config_save_providers", { providers });
}

export function configGetDefaultLlm(): Promise<DefaultLlm> {
  return invoke<DefaultLlm>("config_get_default_llm");
}

export function configSetDefaultLlm(provider: string, model: string): Promise<void> {
  return invoke<void>("config_set_default_llm", { provider, model });
}

export function projectFolderPick(): Promise<string | null> {
  return invoke<string | null>("project_folder_pick");
}

export function openTarget(target?: string, path?: string): Promise<void> {
  return invoke<void>("open_target", { target, path });
}

export function workspaceTrust(trusted: boolean): Promise<boolean> {
  return invoke<boolean>("workspace_trust", { trusted });
}

export function importAiSessions(): Promise<ImportAiSessionsResult> {
  return invoke<ImportAiSessionsResult>("import_ai_sessions");
}

export function licenseNotices(): Promise<unknown[]> {
  return invoke<unknown[]>("license_notices");
}

export function checkForUpdates(): Promise<UpdateCheckResult | null> {
  return invoke<UpdateCheckResult | null>("check_for_updates");
}
