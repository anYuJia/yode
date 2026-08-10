import { describe, expect, it } from "vitest";

import {
  formatDuration,
  runInspectorViewModel,
  runStatusLabel,
  truncateBody
} from "../components/RunInspector";
import type { RunState } from "../lib/desktopTypes";

function run(status: RunState["status"], overrides: Partial<RunState> = {}): RunState {
  return {
    sessionId: "session-1",
    turnId: "turn-1",
    status,
    updatedAt: "2026-08-08T00:00:00Z",
    ...overrides
  };
}

const replayState = (status: "idle" | "loading" | "done" | "error", error?: string) => ({
  status,
  error
});

describe("RunInspector view model", () => {
  it("shows the loading state while replaying", () => {
    const model = runInspectorViewModel(null, false, replayState("loading"), true);
    expect(model.replayLoading).toBe(true);
    expect(model.replayError).toBeNull();
  });

  it("shows the running state with a live label", () => {
    const model = runInspectorViewModel(run("running"), true, replayState("done"), true);
    expect(model.statusLabel).toBe("运行中");
    expect(model.waitingReason).toBeNull();
    expect(model.replayLoading).toBe(false);
  });

  it("shows the waiting reason for approval and user questions", () => {
    expect(runInspectorViewModel(run("waiting_approval"), true, replayState("done"), true).waitingReason).toBe(
      "等待工具执行授权"
    );
    expect(runInspectorViewModel(run("waiting_user"), true, replayState("done"), true).waitingReason).toBe(
      "等待用户回答问题"
    );
    expect(runInspectorViewModel(run("waiting_user"), true, replayState("done"), false).waitingReason).toBe(
      "awaiting your answer"
    );
    expect(runInspectorViewModel(run("cancelling"), true, replayState("done"), true).waitingReason).toBe(
      "正在停止本轮运行"
    );
  });

  it("shows the failed state with diagnostics", () => {
    const model = runInspectorViewModel(
      run("failed", { detail: "引擎崩溃", errorCode: "run_failed" }),
      false,
      replayState("done"),
      true
    );
    expect(model.statusLabel).toBe("失败");
    expect(model.showDiagnostics).toBe(true);
    expect(model.errorCode).toBe("run_failed");
    expect(model.detail).toBe("引擎崩溃");
  });

  it("shows interrupted diagnostics without pretending completion", () => {
    const model = runInspectorViewModel(
      run("interrupted", { detail: "检测到上次运行未正常结束，已标记为中断", errorCode: "interrupted_on_startup" }),
      false,
      replayState("done"),
      true
    );
    expect(model.statusLabel).toBe("已中断");
    expect(model.showDiagnostics).toBe(true);
  });

  it("recovers to the idle state after replay completes", () => {
    const model = runInspectorViewModel(null, false, replayState("done"), true);
    expect(model.statusLabel).toBe("空闲");
    expect(model.replayLoading).toBe(false);
    expect(model.replayError).toBeNull();
  });

  it("keeps the recovery error visible with a retry path", () => {
    const model = runInspectorViewModel(
      run("running"),
      true,
      replayState("error", "重放会话 1 turn 2 失败: 查询失败"),
      true
    );
    expect(model.replayError).toContain("查询失败");
  });

  it("maps every persisted status to a human label", () => {
    for (const status of [
      "starting",
      "running",
      "waiting_approval",
      "waiting_user",
      "cancelling",
      "completed",
      "cancelled",
      "failed",
      "interrupted"
    ] as const) {
      expect(runStatusLabel(status, true).length).toBeGreaterThan(0);
      expect(runStatusLabel(status, false).length).toBeGreaterThan(0);
    }
  });
});

describe("RunInspector event display helpers", () => {
  it("truncates long event bodies safely without leaking tail content", () => {
    const short = "short body";
    expect(truncateBody(short)).toBe(short);
    const long = "x".repeat(1000);
    const truncated = truncateBody(long);
    expect(truncated.length).toBeLessThan(500);
    expect(truncated.endsWith("…")).toBe(true);
  });

  it("formats durations for just-started, minutes and mixed durations", () => {
    expect(formatDuration(0, true)).toBe("刚刚开始");
    expect(formatDuration(0, false)).toBe("just started");
    expect(formatDuration(42_000, true)).toBe("42 秒");
    expect(formatDuration(120_000, true)).toBe("2 分钟");
    expect(formatDuration(125_000, true)).toBe("2 分 5 秒");
    expect(formatDuration(125_000, false)).toBe("2m 5s");
  });
});
