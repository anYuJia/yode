import type { Page } from "playwright/test";

/**
 * 在真实 Chromium 里模拟 Tauri WebView 桥：
 * - window.__TAURI_INTERNALS__.invoke / transformCallback / unregisterCallback；
 * - plugin:event|listen / unlisten 事件总线（emit 通过 window.__YODE_E2E__ 触发）；
 * - 可配置的后端状态：bootstrap 会话、runs（含未终态 turn）、持久化事件、
 *   消息分页窗口；turn_send_message / turn_cancel 按真实节奏发射事件流。
 */

export type MockTurnEvent = {
  sessionId: string;
  turnId: string;
  seq: number;
  kind: string;
  timestamp: string;
  payload: Record<string, unknown>;
};

export type MockBackendState = {
  appVersion: string;
  workspacePath: string;
  provider: string;
  model: string;
  permissionMode: string;
  sessions: Array<{
    id: string;
    title: string;
    projectRoot?: string | null;
    provider?: string;
    model?: string;
    updatedAt: string;
    active?: boolean;
  }>;
  runs: Array<{
    sessionId: string;
    turnId: string;
    status: string;
    updatedAt: string;
    startedAt?: string | null;
    endedAt?: string | null;
    lastSeq?: number;
    detail?: string | null;
    errorCode?: string | null;
  }>;
  /** sessionId + ":" + turnId → 已持久化事件（按 seq 升序）。 */
  turnEvents: Record<string, MockTurnEvent[]>;
  /** sessionId → 历史消息（升序）。 */
  messages: Record<string, Array<Record<string, unknown>>>;
};

export function mockBackend(page: Page, state: MockBackendState) {
  return page.addInitScript((initial: MockBackendState) => {
    const callbacks = new Map<number, { callback: (message: unknown) => void }>();
    let callbackCounter = 0;
    let eventIdCounter = 0;
    const listeners = new Map<string, Array<{ eventId: number; handlerId: number }>>();
    const getListenerHandler = (handlerId: number) => {
      const entry = callbacks.get(handlerId);
      return entry ? entry.callback : null;
    };

    const emit = (event: string, payload: unknown) => {
      for (const registered of listeners.get(event) ?? []) {
        const handler = getListenerHandler(registered.handlerId);
        if (handler) {
          handler({ event, id: registered.eventId, payload });
        }
      }
    };

    const delay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

    const streamTurn = async (
      sessionId: string,
      turnId: string,
      provider: string,
      model: string
    ) => {
      // 真实节奏：turn_started → 推理增量 → 文本增量 → 工具 → 终态
      const ts = () => new Date().toISOString();
      let seq = 0;
      emit("desktop-event", {
        schemaVersion: 1,
        sessionId,
        turnId,
        seq: seq++,
        kind: "turn_started",
        timestamp: ts(),
        payload: { title: "思考中", body: "" }
      });
      await delay(30);
      emit("desktop-event", {
        schemaVersion: 1,
        sessionId,
        turnId,
        seq: seq++,
        kind: "assistant_reasoning_delta",
        timestamp: ts(),
        payload: { reasoning: "让我先分析一下需求。" }
      });
      await delay(30);
      emit("desktop-event", {
        schemaVersion: 1,
        sessionId,
        turnId,
        seq: seq++,
        kind: "assistant_text_delta",
        timestamp: ts(),
        payload: { body: "正在生成回复" }
      });
      await delay(30);
      emit("desktop-event", {
        schemaVersion: 1,
        sessionId,
        turnId,
        seq: seq++,
        kind: "tool_started",
        timestamp: ts(),
        payload: { id: `call-${turnId}`, tool: "bash", title: "调用工具: bash", body: "echo hi", status: "running" }
      });
      await delay(30);
      emit("desktop-event", {
        schemaVersion: 1,
        sessionId,
        turnId,
        seq: seq++,
        kind: "tool_result",
        timestamp: ts(),
        payload: { id: `call-${turnId}`, tool: "bash", title: "工具返回: bash", body: "hi", status: "success" }
      });
      await delay(30);
      // 真实后端先落库再发事件：终态前更新 runs
      initial.runs = initial.runs.map((r) =>
        r.sessionId === sessionId && r.turnId === turnId
          ? { ...r, status: "completed", endedAt: ts() }
          : r
      );
      emit("desktop-event", {
        schemaVersion: 1,
        sessionId,
        turnId,
        seq: seq++,
        kind: "turn_completed",
        timestamp: ts(),
        payload: {
          status: "completed",
          body: "已完成回复",
          reasoning: "让我先分析一下需求。",
          hasToolCalls: true,
          toolCallCount: 1,
          model,
          inputTokens: 10,
          outputTokens: 5,
          totalTokens: 15,
          contextPercent: 5
        }
      });
      void provider;
    };

    // 每个 session 的 turn 发射轨道（避免同会话并发）
    const turnStreams = new Set<string>();

    // 供测试驱动的导出句柄
    (window as unknown as Record<string, unknown>).__YODE_E2E__ = {
      emit,
      turnEvents: initial.turnEvents,
      runs: initial.runs,
      messages: initial.messages
    };

    (window as unknown as Record<string, unknown>).isTauri = true;

    // @tauri-apps/api 的 event 模块在卸载监听器时依赖该内部句柄
    (window as unknown as Record<string, unknown>).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener: () => {},
      registerListener: () => {}
    };

    const invoke = (cmd: string, args: Record<string, unknown>) => {
      switch (cmd) {
        case "plugin:event|listen": {
          const event = String(args.event);
          const handlerId = Number(args.handler);
          const eventId = ++eventIdCounter;
          if (!listeners.has(event)) listeners.set(event, []);
          listeners.get(event)!.push({ eventId, handlerId });
          return Promise.resolve(eventId);
        }
        case "plugin:event|unlisten": {
          const event = String(args.event);
          const eventId = Number(args.eventId);
          const registered = listeners.get(event) ?? [];
          listeners.set(
            event,
            registered.filter((entry) => entry.eventId !== eventId)
          );
          return Promise.resolve(undefined);
        }
        case "app_get_bootstrap": {
          return Promise.resolve({
            appVersion: initial.appVersion,
            workspacePath: initial.workspacePath,
            workspaceTrusted: false,
            provider: initial.provider,
            model: initial.model,
            permissionMode: initial.permissionMode,
            effectivePermissionMode: initial.permissionMode,
            sessions: initial.sessions,
            runs: initial.runs
          });
        }
        case "runs_list": {
          return Promise.resolve(initial.runs);
        }
        case "sessions_list": {
          return Promise.resolve(initial.sessions);
        }
        case "sessions_messages_page": {
          const sessionId = String(args.sessionId ?? args.session_id);
          const before = args.before as number | null | undefined;
          const limit = Number(args.limit ?? 100);
          const all = initial.messages[sessionId] ?? [];
          const windowed = before == null ? all : all.filter((m) => Number(m.sortOrder) < before);
          const page = windowed.slice(-limit).reverse();
          const hasMore = windowed.length > limit;
          return Promise.resolve({ messages: page, hasMore });
        }
        case "sessions_messages": {
          const sessionId = String(args.sessionId ?? args.session_id);
          return Promise.resolve(initial.messages[sessionId] ?? []);
        }
        case "turn_events_since": {
          const sessionId = String(args.sessionId ?? args.session_id);
          const turnId = String(args.turnId ?? args.turn_id);
          const sinceSeq = Number(args.sinceSeq ?? args.since_seq ?? -1);
          const events = initial.turnEvents[`${sessionId}:${turnId}`] ?? [];
          return Promise.resolve(events.filter((e) => e.seq > sinceSeq));
        }
        case "turn_recent_events": {
          const sessionId = String(args.sessionId ?? args.session_id);
          const turnId = String(args.turnId ?? args.turn_id);
          const limit = Number(args.limit ?? 20);
          const events = initial.turnEvents[`${sessionId}:${turnId}`] ?? [];
          return Promise.resolve(events.slice(-limit));
        }
        case "turn_send_message": {
          const request = args.request as {
            sessionId?: string;
            content: string;
            provider?: string;
            model?: string;
            title?: string;
            projectRoot?: string | null;
          };
          const sessionId = request.sessionId ?? `session-${Date.now()}`;
          const turnId = `turn-${Date.now()}`;
          const session = {
            id: sessionId,
            title: request.title ?? request.content.slice(0, 24),
            projectRoot: request.projectRoot ?? null,
            provider: request.provider ?? initial.provider,
            model: request.model ?? initial.model,
            updatedAt: new Date().toISOString(),
            active: true
          };
          const key = `${sessionId}:${turnId}`;
          initial.turnEvents[key] = [];
          initial.runs = [
            {
              sessionId,
              turnId,
              status: "running",
              updatedAt: new Date().toISOString(),
              startedAt: new Date().toISOString(),
              lastSeq: 0
            },
            ...initial.runs.filter((r) => r.sessionId !== sessionId)
          ];
          if (!turnStreams.has(sessionId)) {
            turnStreams.add(sessionId);
            void streamTurn(sessionId, turnId, request.provider ?? initial.provider, request.model ?? initial.model);
          }
          return Promise.resolve({ sessionId, turnId, session });
        }
        case "turn_cancel": {
          const sessionId = String(args.sessionId);
          const turnId = String(args.turnId);
          const key = `${sessionId}:${turnId}`;
          emit("desktop-event", {
            schemaVersion: 1,
            sessionId,
            turnId,
            seq: (initial.turnEvents[key]?.length ?? 0) + 100,
            kind: "cancelling",
            timestamp: new Date().toISOString(),
            payload: { title: "正在取消", body: "正在停止本轮运行。" }
          });
          setTimeout(() => {
            // 真实后端先落库再发事件：终态前更新 runs
            initial.runs = initial.runs.map((r) =>
              r.sessionId === sessionId && r.turnId === turnId
                ? { ...r, status: "cancelled", endedAt: new Date().toISOString() }
                : r
            );
            emit("desktop-event", {
              schemaVersion: 1,
              sessionId,
              turnId,
              seq: (initial.turnEvents[key]?.length ?? 0) + 101,
              kind: "cancelled",
              timestamp: new Date().toISOString(),
              payload: { title: "已取消", body: "本轮运行已停止。" }
            });
          }, 80);
          return Promise.resolve(undefined);
        }
        case "permission_mode_set": {
          return Promise.resolve({
            effectivePermissionMode: initial.permissionMode,
            scope: "user",
            persisted: false,
            bypassActive: false
          });
        }
        case "config_get_providers": {
          return Promise.resolve([]);
        }
        default: {
          return Promise.resolve(undefined);
        }
      }
    };

    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args ?? {}),
      transformCallback: (callback: (message: unknown) => void, once = false) => {
        const id = ++callbackCounter;
        // 与真实 Tauri 一致：回调持久存在，直到 once=true 触发或 unregisterCallback
        callbacks.set(id, {
          callback: (message) => {
            if (once) {
              callbacks.delete(id);
            }
            callback(message);
          }
        });
        return id;
      },
      unregisterCallback: (id: number) => {
        callbacks.delete(id);
      }
    };
  }, state);
}
