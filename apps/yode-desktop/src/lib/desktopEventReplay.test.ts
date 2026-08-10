import { afterEach, describe, expect, it, vi } from "vitest";

import { replayTurnEvents, shouldReplayRun } from "./desktopEventReplay";
import { handleDesktopRuntimeEvent, resetDesktopEventFiltersForTest } from "./desktopEventHandlers";
import type { RunState, TimelineItem, UsageSnapshot } from "./desktopTypes";
import * as desktopIpc from "./desktopIpc";

function run(status: RunState["status"], overrides: Partial<RunState> = {}): RunState {
  return {
    sessionId: "session-1",
    turnId: "turn-1",
    status,
    updatedAt: "2026-08-08T00:00:00Z",
    ...overrides
  };
}

function persistedEvent(
  sessionId: string,
  turnId: string,
  seq: number,
  kind: string,
  payload: Record<string, unknown> = {}
) {
  return { sessionId, turnId, seq, kind, timestamp: "2026-08-08T00:00:00Z", payload };
}

function handlerContext(overrides: Record<string, unknown> = {}) {
  let timeline: TimelineItem[] = [];
  const context = {
    activeSessionId: "session-1",
    currentTurnId: "turn-1",
    getCurrentTurnId: (sessionId: string) => (sessionId === "session-1" ? "turn-1" : null),
    payload: {},
    sendSystemNotification: vi.fn(),
    setCurrentTurnId: vi.fn(),
    setIsProcessing: vi.fn(),
    setPendingUserQuestion: vi.fn(),
    setTimelineItems: vi.fn((updater: (items: TimelineItem[]) => TimelineItem[]) => {
      timeline = updater(timeline);
    }),
    setUsageSnapshot: vi.fn((_updater: (current: UsageSnapshot | null) => UsageSnapshot | null) => {}),
    ...overrides
  };
  return { context, getTimeline: () => timeline };
}

function replayHarness(eventsSince: Array<ReturnType<typeof persistedEvent>>) {
  const spy = vi.spyOn(desktopIpc, "turnEventsSince").mockResolvedValue(eventsSince);
  const { context, getTimeline } = handlerContext();
  const dispatch = (payload: unknown) => handleDesktopRuntimeEvent({ ...context, payload });
  return { spy, dispatch, getTimeline, restore: () => spy.mockRestore() };
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  resetDesktopEventFiltersForTest();
});

describe("event replay", () => {
  it("only replays non-terminal turns", () => {
    for (const status of ["starting", "running", "waiting_approval", "waiting_user", "cancelling"] as const) {
      expect(shouldReplayRun(run(status))).toBe(true);
    }
    for (const status of ["completed", "cancelled", "failed", "interrupted"] as const) {
      expect(shouldReplayRun(run(status))).toBe(false);
    }
  });

  it("replays events idempotently: a second replay never duplicates the timeline", async () => {
    const events = [
      persistedEvent("session-1", "turn-1", 0, "turn_started", { title: "思考中", body: "" }),
      persistedEvent("session-1", "turn-1", 1, "assistant_text_delta", { body: "hello" }),
      persistedEvent("session-1", "turn-1", 2, "assistant_text_delta", { body: " world" }),
      persistedEvent("session-1", "turn-1", 3, "tool_started", {
        id: "c1",
        tool: "bash",
        title: "t",
        body: "cmd",
        status: "running"
      })
    ];
    const harness = replayHarness(events);
    const outcome = await replayTurnEvents({ runs: [run("running", { lastSeq: -1 })], dispatch: harness.dispatch });
    expect(outcome.ok).toBe(true);
    expect(outcome.replayedEvents).toBe(events.length);
    expect(harness.getTimeline().length).toBeGreaterThan(0);

    // 第二次重放：事件轨道按 seq 去重，时间线不重复
    const before = harness.getTimeline().length;
    const second = await replayTurnEvents({ runs: [run("running", { lastSeq: 3 })], dispatch: harness.dispatch });
    expect(second.ok).toBe(true);
    expect(harness.getTimeline().length).toBe(before);
    harness.restore();
  });

  it("keeps a terminal turn frozen against late replay events", async () => {
    const harness = replayHarness([]);
    const { context } = handlerContext();
    const dispatch = (payload: unknown) => handleDesktopRuntimeEvent({ ...context, payload });

    // 终态后的迟到事件（done 之后再来的 delta）不得渲染进时间线
    dispatch(persistedEvent("session-1", "turn-1", 0, "turn_started", {}));
    dispatch(persistedEvent("session-1", "turn-1", 1, "done", {}));
    const before = harness.getTimeline().length;
    dispatch(persistedEvent("session-1", "turn-1", 5, "assistant_text_delta", { body: "late" }));
    expect(harness.getTimeline().length).toBe(before);
    expect(
      harness.getTimeline().some((item) => "body" in item && item.body === "late")
    ).toBe(false);
    harness.restore();
  });

  it("reports a failed query while keeping the caller in charge of locking", async () => {
    const spy = vi.spyOn(desktopIpc, "turnEventsSince").mockRejectedValue(new Error("查询失败"));
    const { context } = handlerContext();
    const outcome = await replayTurnEvents({
      runs: [run("running", { lastSeq: 0 })],
      dispatch: (payload) => handleDesktopRuntimeEvent({ ...context, payload })
    });
    expect(outcome.ok).toBe(false);
    expect(outcome.error).toContain("查询失败");
    expect(outcome.error).toContain("session-1");
    spy.mockRestore();
  });

  it("continues replaying other turns when one turn fails", async () => {
    const spy = vi
      .spyOn(desktopIpc, "turnEventsSince")
      .mockImplementation((sessionId: string) =>
        sessionId === "session-broken"
          ? Promise.reject(new Error("坏会话"))
          : Promise.resolve([persistedEvent(sessionId, "turn-1", 0, "turn_started", {})])
      );
    const { context } = handlerContext();
    const outcome = await replayTurnEvents({
      runs: [
        run("running", { sessionId: "session-broken", lastSeq: -1 }),
        run("running", { sessionId: "session-fine", lastSeq: -1 })
      ],
      dispatch: (payload) => handleDesktopRuntimeEvent({ ...context, payload })
    });
    expect(outcome.ok).toBe(false);
    expect(outcome.replayedTurns).toBe(1);
    expect(outcome.replayedEvents).toBe(1);
    spy.mockRestore();
  });

  it("ignores unknown event kinds during replay instead of crashing", async () => {
    const events = [
      persistedEvent("session-1", "turn-1", 0, "turn_started", {}),
      persistedEvent("session-1", "turn-1", 1, "future_kind", { body: "x" })
    ];
    const harness = replayHarness(events);
    const outcome = await replayTurnEvents({ runs: [run("running", { lastSeq: -1 })], dispatch: harness.dispatch });
    // 未知 kind 被保留到诊断，不导致失败，也不渲染
    expect(outcome.ok).toBe(true);
    harness.restore();
  });
});

describe("cancellation vs replay race", () => {
  it("drops replayed events once a cancellation has been observed for the turn", async () => {
    const harness = replayHarness([]);
    const { context } = handlerContext();
    const dispatch = (payload: unknown) => handleDesktopRuntimeEvent({ ...context, payload });

    // 取消流程已开始（cancelling），随后取消被后端确认（cancelled）
    dispatch(persistedEvent("session-1", "turn-1", 0, "turn_started", {}));
    dispatch(persistedEvent("session-1", "turn-1", 1, "cancelling", {}));
    dispatch(persistedEvent("session-1", "turn-1", 2, "cancelled", {}));

    // 迟到的重放事件（seq 3+）不得复活时间线
    const before = harness.getTimeline().length;
    dispatch(persistedEvent("session-1", "turn-1", 3, "tool_started", { id: "c-late", tool: "bash", title: "t", body: "late", status: "running" }));
    dispatch(persistedEvent("session-1", "turn-1", 4, "assistant_text_delta", { body: "late text" }));
    expect(harness.getTimeline().length).toBe(before);
    expect(
      harness.getTimeline().some((item) => "body" in item && item.body === "late text")
    ).toBe(false);
    harness.restore();
  });

  it("keeps the watchdog decision gated on the same session+turn backend terminal", async () => {
    const { cancellationWatchdogDecision } = await import("./desktopEventHandlers");
    const cancelled = { sessionId: "session-1", turnId: "turn-1", status: "cancelled" as const, updatedAt: "2026-08-08T00:00:00Z" };
    // 同 session 同 turn 的终态 → 释放
    expect(
      cancellationWatchdogDecision({ currentTurnId: "turn-1", sessionId: "session-1", turnId: "turn-1", runs: [cancelled] })
    ).toBe("release");
    // 同 session 不同 turn 的终态 → 不释放
    expect(
      cancellationWatchdogDecision({
        currentTurnId: "turn-1",
        sessionId: "session-1",
        turnId: "turn-1",
        runs: [{ ...cancelled, turnId: "turn-2" }]
      })
    ).toBe("wait");
    // 后端仍 running → 不释放
    expect(
      cancellationWatchdogDecision({
        currentTurnId: "turn-1",
        sessionId: "session-1",
        turnId: "turn-1",
        runs: [{ ...cancelled, status: "running" }]
      })
    ).toBe("wait");
  });
});
