import { describe, expect, it, vi } from "vitest";

import { handleDesktopRuntimeEvent } from "./desktopEventHandlers";
import { TimelineItem } from "./mock";
import { UsageSnapshot } from "./localSlashCommands";

function handlerContext(overrides: Record<string, unknown> = {}) {
  let usage: UsageSnapshot | null = null;
  let timeline: TimelineItem[] = [];
  const context = {
    activeSessionId: "session-1",
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

  it("ignores events for inactive sessions", () => {
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

    expect(context.setUsageSnapshot).not.toHaveBeenCalled();
    expect(context.setTimelineItems).not.toHaveBeenCalled();
  });

  it("sets pending user question for ask_user events", () => {
    const { context } = handlerContext({
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
      })
    );
  });
});
