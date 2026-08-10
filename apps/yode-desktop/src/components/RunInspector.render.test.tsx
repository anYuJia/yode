// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act } from "react-dom/test-utils";
import { createRoot } from "react-dom/client";
import React from "react";

import { RunInspector } from "./RunInspector";
import type { RunState, TimelineItem, TurnEventRecord } from "../lib/desktopTypes";
import * as desktopIpc from "../lib/desktopIpc";

function run(status: RunState["status"], overrides: Partial<RunState> = {}): RunState {
  return {
    sessionId: "session-1",
    turnId: "turn-1",
    status,
    updatedAt: "2026-08-08T00:00:00Z",
    ...overrides
  };
}

function baseProps(overrides: Record<string, unknown> = {}) {
  return {
    isProcessing: false,
    permissionMode: "default",
    timelineItems: [] as TimelineItem[],
    usageSnapshot: null,
    appLang: "zh",
    currentRun: null,
    replayState: { status: "done" as const },
    onRetryReplay: vi.fn(),
    ...overrides
  };
}

function renderInspector(props: Record<string, unknown>) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);
  act(() => {
    root.render(React.createElement(RunInspector, props as never));
  });
  return {
    container,
    text: () => container.textContent ?? "",
    unmount: () => {
      act(() => root.unmount());
      container.remove();
    }
  };
}

function persistedEvent(seq: number, kind: string, payload: Record<string, unknown> = {}): TurnEventRecord {
  return {
    sessionId: "session-1",
    turnId: "turn-1",
    seq,
    kind,
    timestamp: "2026-08-08T00:00:00Z",
    payload
  };
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = "";
});

describe("RunInspector DOM rendering", () => {
  it("renders the replay loading state while recovering", () => {
    vi.spyOn(desktopIpc, "turnRecentEvents").mockResolvedValue([]);
    const { text, unmount } = renderInspector(baseProps({ replayState: { status: "loading" } }));
    expect(text()).toContain("正在恢复");
    expect(text()).toContain("正在重放上次运行事件");
    unmount();
  });

  it("renders the running state with live status", () => {
    vi.spyOn(desktopIpc, "turnRecentEvents").mockResolvedValue([]);
    const { text, unmount } = renderInspector(
      baseProps({ isProcessing: true, currentRun: run("running") })
    );
    expect(text()).toContain("运行中");
    unmount();
  });

  it("renders the waiting reason for approval and user questions", () => {
    vi.spyOn(desktopIpc, "turnRecentEvents").mockResolvedValue([]);
    const approval = renderInspector(baseProps({ currentRun: run("waiting_approval") }));
    expect(approval.text()).toContain("等待工具执行授权");
    approval.unmount();

    const question = renderInspector(baseProps({ currentRun: run("waiting_user") }));
    expect(question.text()).toContain("等待用户回答问题");
    question.unmount();
  });

  it("renders the failed state with error diagnostics", () => {
    vi.spyOn(desktopIpc, "turnRecentEvents").mockResolvedValue([]);
    const { text, unmount } = renderInspector(
      baseProps({
        currentRun: run("failed", { detail: "引擎崩溃", errorCode: "run_failed" })
      })
    );
    expect(text()).toContain("失败");
    expect(text()).toContain("run_failed");
    expect(text()).toContain("引擎崩溃");
    unmount();
  });

  it("renders interrupted diagnostics without faking completion", () => {
    vi.spyOn(desktopIpc, "turnRecentEvents").mockResolvedValue([]);
    const { text, unmount } = renderInspector(
      baseProps({
        currentRun: run("interrupted", {
          detail: "检测到上次运行未正常结束，已标记为中断",
          errorCode: "interrupted_on_startup"
        })
      })
    );
    expect(text()).toContain("已中断");
    expect(text()).toContain("interrupted_on_startup");
    unmount();
  });

  it("renders the recovered idle state and run metadata (started/duration)", () => {
    vi.spyOn(desktopIpc, "turnRecentEvents").mockResolvedValue([]);
    const startedAt = new Date(Date.now() - 65_000).toISOString();
    const { text, unmount } = renderInspector(
      baseProps({ currentRun: run("completed", { startedAt, endedAt: new Date().toISOString() }) })
    );
    expect(text()).toContain("已完成");
    expect(text()).toContain("1 分 5 秒");
    unmount();
  });

  it("shows the recovery error banner with a retry button", () => {
    vi.spyOn(desktopIpc, "turnRecentEvents").mockResolvedValue([]);
    const onRetryReplay = vi.fn();
    const { text, container, unmount } = renderInspector(
      baseProps({
        currentRun: run("running"),
        replayState: { status: "error", error: "重放会话 x turn y 失败: 查询失败" },
        onRetryReplay
      })
    );
    expect(text()).toContain("恢复失败");
    expect(text()).toContain("查询失败");
    const button = container.querySelector("button.inspector-action-button");
    expect(button).not.toBeNull();
    act(() => button?.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect(onRetryReplay).toHaveBeenCalledTimes(1);
    unmount();
  });

  it("loads and renders recent journal events with kind/seq and truncates long bodies", async () => {
    const events = [
      persistedEvent(0, "turn_started", { body: "思考中" }),
      persistedEvent(1, "tool_started", { body: "x".repeat(1200), tool: "bash" })
    ];
    const spy = vi.spyOn(desktopIpc, "turnRecentEvents").mockResolvedValue(events);
    const { text, container, unmount } = renderInspector(baseProps({ currentRun: run("running") }));
    await act(async () => {
      await Promise.resolve();
    });
    expect(spy).toHaveBeenCalledWith("session-1", "turn-1", 20);
    expect(text()).toContain("turn_started");
    expect(text()).toContain("#0");
    expect(text()).toContain("#1");
    // 长事件体被安全截断（不允许泄漏完整内容）
    expect(text()).toContain("…");
    expect(text()).not.toContain("x".repeat(1200));
    unmount();
  });

  it("shows a retryable failure state when event loading fails", async () => {
    vi.spyOn(desktopIpc, "turnRecentEvents").mockRejectedValue(new Error("读取失败"));
    const { text, unmount } = renderInspector(baseProps({ currentRun: run("completed") }));
    await act(async () => {
      await Promise.resolve();
    });
    expect(text()).toContain("事件读取失败");
    expect(text()).toContain("重试");
    unmount();
  });
});
