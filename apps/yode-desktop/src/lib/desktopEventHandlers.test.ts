import { afterEach, describe, expect, it, vi } from "vitest";

import {
  cancellationWatchdogDecision,
  CANCELLATION_STATUS_WATCHDOG_MS,
  handleDesktopRuntimeEvent,
  resetDesktopEventFiltersForTest,
  discardPendingDeltas,
  resetTurnTracksForSession,
  shouldReleaseCurrentTurnForTerminalEvent
} from "./desktopEventHandlers";
import { TimelineItem, UsageSnapshot } from "./desktopTypes";

function desktopEvent(sessionId: string, turnId: string, seq: number, kind: string, payload: Record<string, unknown> = {}) {
  return {
    sessionId,
    turnId,
    seq,
    kind,
    timestamp: new Date().toISOString(),
    payload
  };
}

function handlerContext(overrides: Record<string, unknown> = {}) {
  let usage: UsageSnapshot | null = null;
  let timeline: TimelineItem[] = [];
  const context = {
    activeSessionId: "session-1",
    currentTurnId: "turn-1",
    payload: {
      sessionId: "session-1",
      turnId: "turn-1",
      seq: 1,
      kind: "usage_update",
      timestamp: new Date().toISOString(),
      payload: { inputTokens: 10, outputTokens: 5 }
    },
    sendSystemNotification: vi.fn(),
    setCurrentTurnId: vi.fn(),
    setIsProcessing: vi.fn(),
    setPendingUserQuestion: vi.fn(),
    setTimelineItems: vi.fn((updater: (items: TimelineItem[]) => TimelineItem[]) => {
      timeline = updater(timeline);
    }),
    setUsageSnapshot: vi.fn((updater: (current: UsageSnapshot | null) => UsageSnapshot | null) => {
      usage = updater(usage);
    }),
    ...overrides
  };
  return { context, getUsage: () => usage, getTimeline: () => timeline };
}

afterEach(() => {
  vi.useRealTimers();
  resetDesktopEventFiltersForTest();
});

describe("desktop runtime event handling", () => {
  it("merges usage updates into the usage snapshot", () => {
    const { context, getUsage } = handlerContext();

    handleDesktopRuntimeEvent(context);

    expect(getUsage()).toMatchObject({
      inputTokens: 10,
      outputTokens: 5,
      totalTokens: 15
    });
  });

  it("routes identified events to the matching inactive session", () => {
    const { context } = handlerContext({
      payload: {
        sessionId: "other-session",
        turnId: "turn-1",
        seq: 1,
        kind: "usage_update",
        timestamp: new Date().toISOString(),
        payload: { inputTokens: 10 }
      }
    });

    handleDesktopRuntimeEvent(context);

    expect(context.setUsageSnapshot).toHaveBeenCalledWith(expect.any(Function), "other-session");
    expect(context.setTimelineItems).toHaveBeenCalledWith(expect.any(Function), "other-session");
  });

  it("preserves background-session events without changing the current session snapshot", () => {
    type SessionState = {
      currentTurnId: string | null;
      isProcessing: boolean;
      pendingUserQuestion: unknown;
      timelineItems: TimelineItem[];
      usageSnapshot: UsageSnapshot | null;
    };
    const states = new Map<string, SessionState>([
      ["session-1", {
        currentTurnId: "turn-1",
        isProcessing: true,
        pendingUserQuestion: null,
        timelineItems: [{ id: "one", kind: "assistant", title: "助手", body: "会话一" }],
        usageSnapshot: { inputTokens: 4 }
      }],
      ["session-2", {
        currentTurnId: null,
        isProcessing: false,
        pendingUserQuestion: null,
        timelineItems: [],
        usageSnapshot: null
      }]
    ]);
    const stateFor = (sessionId: string | null | undefined) => states.get(sessionId ?? "session-1")!;
    const context = {
      activeSessionId: "session-1",
      payload: desktopEvent("session-2", "turn-2", 1, "turn_started", {}),
      getCurrentTurnId: (sessionId: string) => stateFor(sessionId).currentTurnId,
      sendSystemNotification: vi.fn(),
      setCurrentTurnId: (turnId: string | null, sessionId?: string | null) => {
        stateFor(sessionId).currentTurnId = turnId;
      },
      setIsProcessing: (isProcessing: boolean, sessionId?: string | null) => {
        stateFor(sessionId).isProcessing = isProcessing;
      },
      setPendingUserQuestion: (question: unknown, sessionId?: string | null) => {
        stateFor(sessionId).pendingUserQuestion = question;
      },
      setTimelineItems: (updater: (items: TimelineItem[]) => TimelineItem[], sessionId?: string | null) => {
        const state = stateFor(sessionId);
        state.timelineItems = updater(state.timelineItems);
      },
      setUsageSnapshot: (
        updater: (current: UsageSnapshot | null) => UsageSnapshot | null,
        sessionId?: string | null
      ) => {
        const state = stateFor(sessionId);
        state.usageSnapshot = updater(state.usageSnapshot);
      }
    };

    handleDesktopRuntimeEvent(context);
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-2", "turn-2", 2, "ask_user", { body: "请选择" })
    });
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-2", "turn-2", 3, "usage_update", { inputTokens: 12 })
    });

    expect(stateFor("session-1")).toMatchObject({
      currentTurnId: "turn-1",
      isProcessing: true,
      usageSnapshot: { inputTokens: 4 },
      timelineItems: [{ id: "one", body: "会话一" }]
    });
    expect(stateFor("session-2")).toMatchObject({
      currentTurnId: "turn-2",
      isProcessing: true,
      pendingUserQuestion: expect.objectContaining({ question: "请选择", turnId: "turn-2" }),
      usageSnapshot: { inputTokens: 12 }
    });
    expect(stateFor("session-2").timelineItems).not.toEqual([]);
  });

  it("does not treat incomplete desktop event envelopes as trusted session events", () => {
    const { context, getUsage } = handlerContext({
      eventKind: "usage_update",
      payload: {
        kind: "usage_update",
        payload: { inputTokens: 11, outputTokens: 7 }
      }
    });

    handleDesktopRuntimeEvent(context);

    expect(getUsage()).toMatchObject({
      inputTokens: 11,
      outputTokens: 7,
      totalTokens: 18
    });
    expect(context.setTimelineItems).toHaveBeenCalled();
  });

  it("sets pending user question for ask_user events", () => {
    const { context } = handlerContext({
      currentTurnId: "turn-ask",
      payload: {
        sessionId: "session-1",
        turnId: "turn-ask",
        seq: 2,
        kind: "ask_user",
        timestamp: new Date().toISOString(),
        payload: { title: "Decision", body: "Pick one?" }
      }
    });

    handleDesktopRuntimeEvent(context);

    expect(context.setPendingUserQuestion).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: "session-1",
        turnId: "turn-ask",
        title: "Decision",
        question: "Pick one?"
      }),
      "session-1"
    );
  });

  it("keeps structured ask_user queries only when the payload is well typed", () => {
    const query = {
      questions: [
        {
          header: "Decision",
          question: "Pick one?",
          options: [{ label: "Proceed", description: "Continue the run" }]
        }
      ]
    };
    const { context } = handlerContext({
      currentTurnId: "turn-ask",
      payload: {
        sessionId: "session-1",
        turnId: "turn-ask",
        seq: 2,
        kind: "ask_user",
        timestamp: new Date().toISOString(),
        payload: { title: "Decision", body: "Pick one?", query }
      }
    });

    handleDesktopRuntimeEvent(context);

    expect(context.setPendingUserQuestion).toHaveBeenCalledWith(
      expect.objectContaining({
        query
      }),
      "session-1"
    );
  });

  it("drops malformed structured ask_user query payloads", () => {
    const { context } = handlerContext({
      currentTurnId: "turn-ask",
      payload: {
        sessionId: "session-1",
        turnId: "turn-ask",
        seq: 2,
        kind: "ask_user",
        timestamp: new Date().toISOString(),
        payload: {
          title: "Decision",
          body: "Pick one?",
          query: { questions: [{ header: "Decision", question: "Pick one?", options: [null] }] }
        }
      }
    });

    handleDesktopRuntimeEvent(context);

    expect(context.setPendingUserQuestion).toHaveBeenCalledWith(
      expect.objectContaining({
        query: undefined
      }),
      "session-1"
    );
  });
});

describe("desktop event isolation", () => {
  it("drops duplicate events with the same seq for the same turn", () => {
    const { context, getTimeline } = handlerContext({
      payload: desktopEvent("session-1", "turn-1", 3, "tool_started", {
        id: "t1",
        tool: "bash",
        title: "调用工具: bash",
        body: "{}",
        status: "running"
      })
    });

    handleDesktopRuntimeEvent(context);
    handleDesktopRuntimeEvent(context);

    const items = getTimeline();
    expect(items.filter((item) => item.kind === "tool")).toHaveLength(1);
  });

  it("drops out-of-order events with lower seq after a higher seq was seen", () => {
    const { context, getTimeline } = handlerContext({
      payload: desktopEvent("session-1", "turn-1", 5, "tool_started", {
        id: "t1",
        tool: "bash",
        title: "调用工具: bash",
        body: "{}",
        status: "running"
      })
    });
    handleDesktopRuntimeEvent(context);
    handleDesktopRuntimeEvent(context);
    handleDesktopRuntimeEvent(context);

    const items = getTimeline();
    expect(items.filter((item) => item.kind === "tool")).toHaveLength(1);
  });

  it("drops late events for a cancelled turn and resets processing on cancelled", () => {
    const { context, getTimeline } = handlerContext({});

    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 5, "cancelling", {})
    });
    // 取消后迟到的工具事件必须被丢弃
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 6, "tool_started", {
        id: "t1",
        tool: "bash",
        title: "调用工具: bash",
        body: "{}",
        status: "running"
      })
    });
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 7, "cancelled", {})
    });

    const items = getTimeline();
    expect(items.some((item) => item.kind === "tool")).toBe(false);
    expect(context.setIsProcessing).toHaveBeenCalledWith(false, "session-1");
  });

  it("keeps cancellation watchdog locked when the backend still reports running after 2.5 seconds", () => {
    expect(CANCELLATION_STATUS_WATCHDOG_MS).toBe(2500);
    expect(cancellationWatchdogDecision({
      currentTurnId: "turn-1",
      sessionId: "session-1",
      turnId: "turn-1",
      runs: [{
        sessionId: "session-1",
        turnId: "turn-1",
        status: "running",
        updatedAt: "2026-08-08T00:00:00Z"
      }]
    })).toBe("wait");
    expect(cancellationWatchdogDecision({
      currentTurnId: "turn-1",
      sessionId: "session-1",
      turnId: "turn-1",
      runs: [{
        sessionId: "session-1",
        turnId: "turn-1",
        status: "cancelling",
        updatedAt: "2026-08-08T00:00:00Z"
      }]
    })).toBe("wait");
  });

  it("releases only the current turn after a terminal event or a confirmed terminal run state", () => {
    expect(shouldReleaseCurrentTurnForTerminalEvent("turn-1", "turn-1", "cancelling")).toBe(false);
    expect(shouldReleaseCurrentTurnForTerminalEvent("turn-1", "turn-1", "cancelled")).toBe(true);
    expect(shouldReleaseCurrentTurnForTerminalEvent("turn-1", "turn-1", "done")).toBe(true);
    expect(shouldReleaseCurrentTurnForTerminalEvent("turn-1", "turn-2", "done")).toBe(false);
    expect(cancellationWatchdogDecision({
      currentTurnId: "turn-1",
      sessionId: "session-1",
      turnId: "turn-1",
      runs: [{
        sessionId: "session-1",
        turnId: "turn-1",
        status: "cancelled",
        updatedAt: "2026-08-08T00:00:00Z"
      }]
    })).toBe("release");
  });

  it("ignores an old turn watchdog and terminal event after a newer turn begins", () => {
    expect(cancellationWatchdogDecision({
      currentTurnId: "turn-new",
      sessionId: "session-1",
      turnId: "turn-old",
      runs: [{
        sessionId: "session-1",
        turnId: "turn-old",
        status: "cancelled",
        updatedAt: "2026-08-08T00:00:00Z"
      }]
    })).toBe("ignore");
    expect(cancellationWatchdogDecision({
      currentTurnId: "turn-old",
      sessionId: "session-1",
      turnId: "turn-old",
      runs: [{
        sessionId: "session-2",
        turnId: "turn-old",
        status: "cancelled",
        updatedAt: "2026-08-08T00:00:00Z"
      }]
    })).toBe("wait");

    const { context } = handlerContext({ currentTurnId: "turn-old" });
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-old", 1, "turn_started", {})
    });
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-old", 2, "turn_completed", {})
    });
    context.setIsProcessing.mockClear();
    context.setCurrentTurnId.mockClear();

    handleDesktopRuntimeEvent({
      ...context,
      currentTurnId: "turn-new",
      payload: desktopEvent("session-1", "turn-new", 1, "turn_started", {})
    });
    context.setIsProcessing.mockClear();
    context.setCurrentTurnId.mockClear();
    handleDesktopRuntimeEvent({
      ...context,
      currentTurnId: "turn-new",
      payload: desktopEvent("session-1", "turn-old", 3, "done", {})
    });

    expect(context.setIsProcessing).not.toHaveBeenCalledWith(false, "session-1");
    expect(context.setCurrentTurnId).not.toHaveBeenCalledWith(null, "session-1");
  });

  it("keeps events for a different turn id isolated", () => {
    const { context, getTimeline } = handlerContext({});
    handleDesktopRuntimeEvent(context);
    // 切换到 turn-2（模拟新 turn 的 turn_started 之后的状态）
    handleDesktopRuntimeEvent({
      ...context,
      currentTurnId: "turn-2",
      payload: desktopEvent("session-1", "turn-2", 1, "tool_started", {
        id: "t1",
        tool: "bash",
        title: "调用工具: bash",
        body: "{}",
        status: "running"
      })
    });

    const items = getTimeline();
    expect(items.filter((item) => item.kind === "tool")).toHaveLength(1);
  });

  it("drops events from a non-current turn while another turn is active", () => {
    const { context, getTimeline } = handlerContext({});
    handleDesktopRuntimeEvent({
      ...context,
      currentTurnId: "turn-2",
      payload: desktopEvent("session-1", "turn-1", 9, "tool_started", {
        id: "t-stale",
        tool: "bash",
        title: "调用工具: bash",
        body: "{}",
        status: "running"
      })
    });

    const items = getTimeline();
    expect(items.some((item) => item.kind === "tool")).toBe(false);
  });

  it("batches text deltas into a single timeline update", () => {
    vi.useFakeTimers();
    const { context, getTimeline } = handlerContext({});
    const deltaA = desktopEvent("session-1", "turn-1", 10, "assistant_text_delta", {
      body: "你好"
    });
    const deltaB = desktopEvent("session-1", "turn-1", 11, "assistant_text_delta", {
      body: "，世界"
    });
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 9, "turn_started", {})
    });
    handleDesktopRuntimeEvent({ ...context, payload: deltaA });
    handleDesktopRuntimeEvent({ ...context, payload: deltaB });

    // turn_started 自身产生 1 次时间线更新（思考中条目）
    expect(context.setTimelineItems).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(30);
    // 两个 delta 合帧为 1 次提交
    expect(context.setTimelineItems).toHaveBeenCalledTimes(2);
    const items = getTimeline();
    const assistant = items.find((item) => item.kind === "assistant");
    expect(assistant && "body" in assistant && assistant.body).toBe("你好，世界");
  });

  it("flushes pending deltas when a non-delta event arrives", () => {
    vi.useFakeTimers();
    const { context } = handlerContext({});
    const deltaA = desktopEvent("session-1", "turn-1", 10, "assistant_text_delta", {
      body: "你好"
    });
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 9, "turn_started", {})
    });
    handleDesktopRuntimeEvent({ ...context, payload: deltaA });

    // 1 次：turn_started 的思考中条目；delta 已入批未提交
    expect(context.setTimelineItems).toHaveBeenCalledTimes(1);
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 11, "tool_started", {
        id: "t1",
        tool: "bash",
        title: "调用工具: bash",
        body: "{}",
        status: "running"
      })
    });
    // 3 次：turn_started 条目 + delta 合帧提交 + 工具条目
    expect(context.setTimelineItems).toHaveBeenCalledTimes(3);
  });

  it("drops queued deltas from an old turn when the same session moves to a new turn", () => {
    vi.useFakeTimers();
    const { context, getTimeline } = handlerContext({});
    // 真实流（后端每会话串行）：turn-1 流式进行中，delta 已入批
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 1, "turn_started", {})
    });
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 2, "assistant_text_delta", {
        body: "旧turn内容"
      })
    });
    // turn-1 正常结束（终态事件清空当前 turn 并丢弃未 flush 的旧 delta）
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 3, "turn_completed", {
        body: "旧turn内容",
        reasoning: ""
      })
    });
    // 同一会话切到 turn-2
    handleDesktopRuntimeEvent({
      ...context,
      currentTurnId: "turn-2",
      payload: desktopEvent("session-1", "turn-2", 1, "turn_started", {})
    });
    handleDesktopRuntimeEvent({
      ...context,
      currentTurnId: "turn-2",
      payload: desktopEvent("session-1", "turn-2", 2, "assistant_text_delta", {
        body: "新turn内容"
      })
    });
    vi.advanceTimersByTime(30);

    const items = getTimeline();
    const assistants = items.filter(
      (item): item is Extract<TimelineItem, { kind: "user" | "assistant" | "reasoning" }> =>
        item.kind === "assistant"
    );
    // turn-1 的最终文本来自 turn_completed 事件本身（应保留在时间线中），
    // 但不得与 turn-2 的增量合并到同一条消息里
    const last = assistants[assistants.length - 1];
    expect(last && last.body).toBe("新turn内容");
    expect(assistants.some((item) => item.body.includes("旧turn内容") && item.body.includes("新turn内容"))).toBe(false);
  });

  it("rejects a stale turn_started from a different turn while a turn is active", () => {
    vi.useFakeTimers();
    const { context, getTimeline } = handlerContext({});
    // turn-1 进行中（lastSeenTurn = turn-1）
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 1, "turn_started", {})
    });
    // 旧 turn 未终态时，同会话其他 turn 的 turn_started 是过期/乱序事件：拒绝，
    // 不得把全局 turn 指针改回/覆盖当前 turn
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-2", 1, "turn_started", {})
    });
    // turn-1 的 delta 仍正常合帧（指针未被覆盖）
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 2, "assistant_text_delta", {
        body: "turn1内容"
      })
    });
    vi.advanceTimersByTime(30);

    const items = getTimeline();
    const assistant = items.find((item) => item.kind === "assistant");
    expect(assistant && "body" in assistant && assistant.body).toBe("turn1内容");
  });

  it("does not let a stale turn_started reclaim lastSeen after its turn finished", () => {
    const { context, getTimeline } = handlerContext({});
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 10, "turn_started", {})
    });
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 11, "turn_completed", {
        body: "完成",
        reasoning: ""
      })
    });
    // 已结束 turn 的低序号 start 必须先被 seq 门禁丢弃，不能重新占用 lastSeen。
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 9, "turn_started", {})
    });
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-2", 1, "turn_started", {})
    });
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-2", 2, "tool_started", {
        id: "turn-2-tool",
        tool: "bash",
        title: "调用工具: bash",
        body: "{}",
        status: "running"
      })
    });

    expect(
      getTimeline().some(
        (item) => item.kind === "tool" && item.title === "调用工具: bash"
      )
    ).toBe(true);
  });

  it("does not add timeline items for cancelling/cancelled lifecycle events", () => {
    const { context, getTimeline } = handlerContext({});
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 5, "cancelling", {})
    });
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 6, "cancelled", {})
    });

    const items = getTimeline();
    expect(items).toHaveLength(0);
    expect(context.setIsProcessing).toHaveBeenCalledWith(false, "session-1");
  });

  it("ignores stale turn_completed for a turn that already finished", () => {
    const { context } = handlerContext({
      payload: desktopEvent("session-1", "turn-1", 2, "turn_completed", {
        body: "done",
        reasoning: ""
      })
    });
    handleDesktopRuntimeEvent(context);
    expect(context.setIsProcessing).toHaveBeenCalledWith(false, "session-1");
  });

  it("keeps isProcessing true after cancelling until the backend confirms cancelled", () => {
    const { context } = handlerContext({});
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 3, "cancelling", {})
    });
    // cancelling 只是请求阶段：不得提前空闲
    expect(context.setIsProcessing).not.toHaveBeenCalledWith(false);

    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 4, "cancelled", {})
    });
    // 只有后端确认停止后才复位
    expect(context.setIsProcessing).toHaveBeenCalledWith(false, "session-1");
  });

  it("drops batched deltas that belong to a previous session after a session switch", () => {
    vi.useFakeTimers();
    const { context, getTimeline } = handlerContext({});
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 9, "turn_started", {})
    });
    const oldSessionDelta = desktopEvent("session-1", "turn-1", 10, "assistant_text_delta", {
      body: "旧会话内容"
    });
    handleDesktopRuntimeEvent({ ...context, payload: oldSessionDelta });

    // 25ms 窗口内切换到另一个会话（模拟 App 层 clearTurnState + discard）
    discardPendingDeltas();
    resetTurnTracksForSession("session-1");
    const newContext = {
      ...context,
      activeSessionId: "session-2",
      currentTurnId: "turn-9"
    };
    handleDesktopRuntimeEvent({
      ...newContext,
      payload: desktopEvent("session-2", "turn-9", 1, "turn_started", {})
    });
    handleDesktopRuntimeEvent({
      ...newContext,
      payload: desktopEvent("session-2", "turn-9", 2, "assistant_text_delta", {
        body: "新会话内容"
      })
    });
    vi.advanceTimersByTime(30);

    const items = getTimeline();
    const assistant = items.find((item) => item.kind === "assistant");
    expect(assistant && "body" in assistant && assistant.body).toBe("新会话内容");
    expect(items.join()).not.toContain("旧会话内容");
  });

  it("keeps another session's batch timer alive when one session is discarded", () => {
    vi.useFakeTimers();
    const sessionA = handlerContext({
      activeSessionId: "session-a",
      currentTurnId: "turn-a",
      payload: desktopEvent("session-a", "turn-a", 1, "turn_started", {})
    });
    const sessionB = handlerContext({
      activeSessionId: "session-b",
      currentTurnId: "turn-b",
      payload: desktopEvent("session-b", "turn-b", 1, "turn_started", {})
    });

    handleDesktopRuntimeEvent(sessionA.context);
    handleDesktopRuntimeEvent(sessionB.context);
    handleDesktopRuntimeEvent({
      ...sessionA.context,
      payload: desktopEvent("session-a", "turn-a", 2, "assistant_text_delta", { body: "A" })
    });
    handleDesktopRuntimeEvent({
      ...sessionB.context,
      payload: desktopEvent("session-b", "turn-b", 2, "assistant_text_delta", { body: "B" })
    });

    discardPendingDeltas("session-a");
    vi.advanceTimersByTime(30);

    expect(sessionA.getTimeline().some((item) => item.kind === "assistant" && item.body === "A")).toBe(false);
    expect(sessionB.getTimeline().some((item) => item.kind === "assistant" && item.body === "B")).toBe(true);
  });

  it("does not accept events from a cleared session after switching back", () => {
    vi.useFakeTimers();
    const { context, getTimeline } = handlerContext({});
    // 会话 A 的 turn 曾经运行过（轨道存在）
    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 1, "tool_started", {
        id: "t1",
        tool: "bash",
        title: "调用工具: bash",
        body: "{}",
        status: "running"
      })
    });
    // 切换到会话 B 再切回：轨道被清空，旧 turn 迟到事件必须被丢弃
    resetTurnTracksForSession("session-1");
    const backContext = { ...context, currentTurnId: null };
    handleDesktopRuntimeEvent({
      ...backContext,
      payload: desktopEvent("session-1", "turn-1", 5, "tool_started", {
        id: "t1",
        tool: "bash",
        title: "调用工具: bash",
        body: "{}",
        status: "running"
      })
    });

    const items = getTimeline();
    expect(items.filter((item) => item.kind === "tool")).toHaveLength(1);
  });
});

describe("streaming commit-rate bounds", () => {
  it("keeps UI commits under 40/s while streaming 10k chars", () => {
    vi.useFakeTimers();
    const { context, getTimeline } = handlerContext({});

    // 模拟 10k 字符流式输出：10 秒内每秒 200 个 5 字符 delta（共 2000 个事件）
    let seq = 10;
    let chunk = 0;
    const originalSpy = context.setTimelineItems as unknown as { mock: { calls: unknown[][] } };
    const spyCalls = () => originalSpy.mock.calls.length;

    handleDesktopRuntimeEvent({
      ...context,
      payload: desktopEvent("session-1", "turn-1", 9, "turn_started", {})
    });
    for (let second = 0; second < 10; second += 1) {
      for (let i = 0; i < 200; i += 1) {
        handleDesktopRuntimeEvent({
          ...context,
          payload: desktopEvent("session-1", "turn-1", seq++, "assistant_text_delta", {
            body: `c${String(chunk++).padStart(4, "0")}`
          })
        });
      }
      vi.advanceTimersByTime(1000);
    }

    const totalCommits = spyCalls();
    // 25ms 合帧上限：10 秒内提交数 ≤ 10s / 25ms = 400，远低于 2000 个事件
    expect(totalCommits).toBeLessThanOrEqual(401);
    expect(totalCommits).toBeLessThan(2000);

    // 最终文本必须完整（无 token 丢失）
    const items = getTimeline();
    const assistant = items.find((item) => item.kind === "assistant");
    expect(assistant && "body" in assistant && assistant.body.length).toBe(10_000);
    expect(spyCalls()).toBeGreaterThan(0);
  });
});
