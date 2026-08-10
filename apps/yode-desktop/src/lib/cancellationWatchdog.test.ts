import { afterEach, describe, expect, it, vi } from "vitest";

import {
  CancellationWatchdogOptions,
  nextCancellationPollDelayMs,
  startCancellationWatchdog
} from "./cancellationWatchdog";
import { CANCELLATION_STATUS_WATCHDOG_MS, isTerminalTurnEvent } from "./desktopEventHandlers";
import { RunState } from "./desktopTypes";

function run(sessionId: string, turnId: string, status: RunState["status"]): RunState {
  return { sessionId, turnId, status, updatedAt: "2026-08-08T00:00:00Z" };
}

function harness(overrides: Partial<CancellationWatchdogOptions> = {}) {
  let current = true;
  const fetchRuns = vi.fn<() => Promise<RunState[]>>();
  const onReleased = vi.fn();
  const onWaiting = vi.fn();
  const onQueryError = vi.fn();
  const watchdog = startCancellationWatchdog({
    sessionId: "session-1",
    turnId: "turn-1",
    isStillCurrent: () => current,
    fetchRuns,
    onReleased,
    onWaiting,
    onQueryError,
    schedule: (callback, delayMs) => setTimeout(callback, delayMs),
    clear: (handle) => clearTimeout(handle),
    ...overrides
  });
  return {
    watchdog,
    fetchRuns,
    onReleased,
    onWaiting,
    onQueryError,
    setCurrent: (value: boolean) => {
      current = value;
    }
  };
}

describe("cancellation watchdog polling", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("keeps waiting with backoff while the backend still reports running/cancelling", async () => {
    vi.useFakeTimers();
    const h = harness();
    h.fetchRuns.mockResolvedValue([run("session-1", "turn-1", "cancelling")]);
    h.watchdog.start();

    await vi.advanceTimersByTimeAsync(CANCELLATION_STATUS_WATCHDOG_MS);
    expect(h.fetchRuns).toHaveBeenCalledTimes(1);
    expect(h.onWaiting).toHaveBeenCalledTimes(1);
    expect(h.onReleased).not.toHaveBeenCalled();

    // 退避重试：第 1 次重试等待 1s 后再查，仍是 waiting
    await vi.advanceTimersByTimeAsync(nextCancellationPollDelayMs(1));
    expect(h.fetchRuns).toHaveBeenCalledTimes(2);
    expect(h.onWaiting).toHaveBeenCalledTimes(2);
    expect(h.onReleased).not.toHaveBeenCalled();

    // 第 2 次重试等待 2s
    await vi.advanceTimersByTimeAsync(nextCancellationPollDelayMs(2));
    expect(h.fetchRuns).toHaveBeenCalledTimes(3);
    expect(h.onWaiting).toHaveBeenCalledTimes(3);
  });

  it("releases as soon as the backend reports a terminal state for the same turn", async () => {
    vi.useFakeTimers();
    const h = harness();
    h.fetchRuns.mockResolvedValue([run("session-1", "turn-1", "cancelled")]);
    h.watchdog.start();

    await vi.advanceTimersByTimeAsync(CANCELLATION_STATUS_WATCHDOG_MS);
    expect(h.onReleased).toHaveBeenCalledTimes(1);
    expect(h.onWaiting).not.toHaveBeenCalled();
  });

  it("recovers from query errors and keeps retrying until a terminal state appears", async () => {
    vi.useFakeTimers();
    const h = harness();
    h.fetchRuns
      .mockRejectedValueOnce(new Error("ipc unavailable"))
      .mockResolvedValueOnce([run("session-1", "turn-1", "running")])
      .mockResolvedValue([run("session-1", "turn-1", "cancelled")]);
    h.watchdog.start();

    await vi.advanceTimersByTimeAsync(CANCELLATION_STATUS_WATCHDOG_MS);
    expect(h.onQueryError).toHaveBeenCalledTimes(1);
    expect(h.onReleased).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(nextCancellationPollDelayMs(1));
    expect(h.onWaiting).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(nextCancellationPollDelayMs(2));
    expect(h.onReleased).toHaveBeenCalledTimes(1);
  });

  it("stops polling once the terminal state has been released", async () => {
    vi.useFakeTimers();
    const h = harness();
    h.fetchRuns.mockResolvedValue([run("session-1", "turn-1", "cancelled")]);
    h.watchdog.start();

    await vi.advanceTimersByTimeAsync(CANCELLATION_STATUS_WATCHDOG_MS);
    expect(h.onReleased).toHaveBeenCalledTimes(1);

    const callsAfterRelease = h.fetchRuns.mock.calls.length;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(h.fetchRuns.mock.calls.length).toBe(callsAfterRelease);
  });

  it("never releases when the turn is no longer current (new turn took over)", async () => {
    vi.useFakeTimers();
    const h = harness();
    h.fetchRuns.mockResolvedValue([run("session-1", "turn-1", "cancelled")]);
    h.watchdog.start();
    // 开始新 turn：旧 turn 不再是对应当前 UI 的 turn
    h.setCurrent(false);

    await vi.advanceTimersByTimeAsync(CANCELLATION_STATUS_WATCHDOG_MS + 60_000);
    expect(h.fetchRuns).not.toHaveBeenCalled();
    expect(h.onReleased).not.toHaveBeenCalled();
  });

  it("stops mid-flight polling when the turn loses currency between polls", async () => {
    vi.useFakeTimers();
    const h = harness();
    h.fetchRuns.mockResolvedValue([run("session-1", "turn-1", "cancelling")]);
    h.watchdog.start();

    await vi.advanceTimersByTimeAsync(CANCELLATION_STATUS_WATCHDOG_MS);
    expect(h.onWaiting).toHaveBeenCalledTimes(1);
    // 第一次等待后用户切换到新 turn：不得再发起后续查询或解锁
    h.setCurrent(false);
    await vi.advanceTimersByTimeAsync(nextCancellationPollDelayMs(1) + 10);
    expect(h.fetchRuns).toHaveBeenCalledTimes(1);
    expect(h.onReleased).not.toHaveBeenCalled();
  });

  it("keeps waiting when the only terminal run record belongs to another session/turn", async () => {
    vi.useFakeTimers();
    const h = harness();
    // 跨会话隔离：session-2 的终态运行记录不能解锁 session-1 的 turn-1
    h.fetchRuns.mockResolvedValue([run("session-2", "turn-9", "cancelled")]);
    h.watchdog.start();

    await vi.advanceTimersByTimeAsync(CANCELLATION_STATUS_WATCHDOG_MS);
    expect(h.onWaiting).toHaveBeenCalledTimes(1);
    expect(h.onReleased).not.toHaveBeenCalled();
  });

  it("keeps waiting when the run record is missing instead of assuming success", async () => {
    vi.useFakeTimers();
    const h = harness();
    h.fetchRuns.mockResolvedValue([]);
    h.watchdog.start();

    await vi.advanceTimersByTimeAsync(CANCELLATION_STATUS_WATCHDOG_MS);
    expect(h.onWaiting).toHaveBeenCalledTimes(1);
    expect(h.onReleased).not.toHaveBeenCalled();
  });

  it("stop() cancels any pending timer immediately", async () => {
    vi.useFakeTimers();
    const h = harness();
    h.fetchRuns.mockResolvedValue([run("session-1", "turn-1", "cancelling")]);
    h.watchdog.start();
    h.watchdog.stop();

    await vi.advanceTimersByTimeAsync(60_000);
    expect(h.fetchRuns).not.toHaveBeenCalled();
    expect(h.onReleased).not.toHaveBeenCalled();
  });

  it("start() is idempotent: repeated starts schedule only one initial timer and one request", async () => {
    vi.useFakeTimers();
    const h = harness();
    h.fetchRuns.mockResolvedValue([run("session-1", "turn-1", "cancelling")]);
    // 双击取消/重复武装场景：多次 start 不得产生第二个计时器或重复请求
    h.watchdog.start();
    h.watchdog.start();
    h.watchdog.start();

    await vi.advanceTimersByTimeAsync(CANCELLATION_STATUS_WATCHDOG_MS);
    expect(h.fetchRuns).toHaveBeenCalledTimes(1);
    expect(h.onWaiting).toHaveBeenCalledTimes(1);

    // 首次计时器已触发后再次 start 仍为 no-op
    h.watchdog.start();
    await vi.advanceTimersByTimeAsync(nextCancellationPollDelayMs(1));
    expect(h.fetchRuns).toHaveBeenCalledTimes(2);
  });

  it("start() after stop() is a no-op", async () => {
    vi.useFakeTimers();
    const h = harness();
    h.fetchRuns.mockResolvedValue([run("session-1", "turn-1", "cancelling")]);
    h.watchdog.start();
    h.watchdog.stop();
    h.watchdog.start();

    await vi.advanceTimersByTimeAsync(60_000);
    expect(h.fetchRuns).not.toHaveBeenCalled();
    expect(h.onWaiting).not.toHaveBeenCalled();
  });

  it("stop() while a request is in flight suppresses every callback when it resolves", async () => {
    vi.useFakeTimers();
    const h = harness();
    let resolveFetch!: (runs: RunState[]) => void;
    h.fetchRuns.mockImplementation(
      () => new Promise<RunState[]>((resolve) => { resolveFetch = resolve; })
    );
    h.watchdog.start();

    await vi.advanceTimersByTimeAsync(CANCELLATION_STATUS_WATCHDOG_MS);
    expect(h.fetchRuns).toHaveBeenCalledTimes(1);

    // 请求进行中调用 stop()：请求返回终态也不得触发任何回调
    h.watchdog.stop();
    resolveFetch([run("session-1", "turn-1", "cancelled")]);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(h.onReleased).not.toHaveBeenCalled();
    expect(h.onWaiting).not.toHaveBeenCalled();
    expect(h.onQueryError).not.toHaveBeenCalled();
    // 不产生后续轮询
    expect(h.fetchRuns).toHaveBeenCalledTimes(1);
  });

  it("stop() while a request is in flight suppresses callbacks on rejection too", async () => {
    vi.useFakeTimers();
    const h = harness();
    let rejectFetch!: (err: Error) => void;
    h.fetchRuns.mockImplementation(
      () => new Promise<RunState[]>((_resolve, reject) => { rejectFetch = reject; })
    );
    h.watchdog.start();

    await vi.advanceTimersByTimeAsync(CANCELLATION_STATUS_WATCHDOG_MS);
    h.watchdog.stop();
    rejectFetch(new Error("ipc unavailable"));
    await vi.advanceTimersByTimeAsync(60_000);

    expect(h.onQueryError).not.toHaveBeenCalled();
    expect(h.onReleased).not.toHaveBeenCalled();
    expect(h.fetchRuns).toHaveBeenCalledTimes(1);
  });

  it("applies controlled backoff delays capped at 15 seconds", () => {
    expect(CANCELLATION_STATUS_WATCHDOG_MS).toBe(2500);
    expect(nextCancellationPollDelayMs(1)).toBe(1000);
    expect(nextCancellationPollDelayMs(2)).toBe(2000);
    expect(nextCancellationPollDelayMs(3)).toBe(4000);
    expect(nextCancellationPollDelayMs(4)).toBe(8000);
    expect(nextCancellationPollDelayMs(5)).toBe(15000);
    expect(nextCancellationPollDelayMs(10)).toBe(15000);
    expect(nextCancellationPollDelayMs(0)).toBe(1000);
  });
});

describe("isTerminalTurnEvent", () => {
  it("identifies terminal turn events with their session and turn", () => {
    const base = {
      sessionId: "session-1",
      turnId: "turn-1",
      seq: 5,
      timestamp: "2026-08-08T00:00:00Z",
      payload: {}
    };
    expect(isTerminalTurnEvent({ ...base, kind: "cancelled" })).toEqual({
      sessionId: "session-1",
      turnId: "turn-1"
    });
    expect(isTerminalTurnEvent({ ...base, kind: "done" })).toEqual({
      sessionId: "session-1",
      turnId: "turn-1"
    });
    expect(isTerminalTurnEvent({ ...base, kind: "turn_completed" })).toEqual({
      sessionId: "session-1",
      turnId: "turn-1"
    });
    expect(isTerminalTurnEvent({ ...base, kind: "error" })).toEqual({
      sessionId: "session-1",
      turnId: "turn-1"
    });
  });

  it("returns null for non-terminal or unidentifiable events", () => {
    const base = {
      sessionId: "session-1",
      turnId: "turn-1",
      seq: 5,
      timestamp: "2026-08-08T00:00:00Z",
      payload: {}
    };
    expect(isTerminalTurnEvent({ ...base, kind: "cancelling" })).toBeNull();
    expect(isTerminalTurnEvent({ ...base, kind: "turn_started" })).toBeNull();
    expect(isTerminalTurnEvent({ ...base, kind: "assistant_text_delta" })).toBeNull();
    expect(isTerminalTurnEvent({ ...base, kind: "cancelled", sessionId: undefined })).toBeNull();
    expect(isTerminalTurnEvent({})).toBeNull();
    expect(isTerminalTurnEvent(null)).toBeNull();
  });
});
