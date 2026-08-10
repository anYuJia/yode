import { afterEach, describe, expect, it, vi } from "vitest";

import { CancellationWatchdogRegistry, WatchdogFactory } from "./cancellationWatchdogRegistry";
import { RunState } from "./desktopTypes";

function run(sessionId: string, turnId: string, status: RunState["status"]): RunState {
  return { sessionId, turnId, status, updatedAt: "2026-08-08T00:00:00Z" };
}

function makeFactory(
  state: { current: boolean },
  fetchRuns: ReturnType<typeof vi.fn<() => Promise<RunState[]>>>
): WatchdogFactory {
  return (sessionId, turnId, onConfirmedTerminal) => {
    // 复用真实 watchdog 实现，注入可观测回调与 fake timer 调度
    return {
      start: vi.fn(() => {
        fetchRuns.mockImplementation(() => Promise.resolve([run(sessionId, turnId, "cancelled")]));
      }),
      stop: vi.fn(() => {
        // 模拟真实 watchdog 停止后不再回调
      }),
      _onConfirmedTerminal: onConfirmedTerminal,
      _sessionId: sessionId,
      _turnId: turnId
    } as never;
  };
}

describe("CancellationWatchdogRegistry", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("arms idempotently: double cancel keeps a single watcher registration", () => {
    const registry = new CancellationWatchdogRegistry();
    const fetchRuns = vi.fn<() => Promise<RunState[]>>();
    const factory = makeFactory({ current: true }, fetchRuns);

    registry.markPending("session-1", "turn-1");
    const first = registry.arm("session-1", "turn-1", factory);
    const second = registry.arm("session-1", "turn-1", factory);

    expect(second).toBe(first);
    expect(registry.watchdogKeys()).toEqual(["session-1:turn-1"]);
    expect(registry.pendingKeys()).toEqual(["session-1:turn-1"]);
  });

  it("cleans both watcher and pending registrations when the terminal event arrives", () => {
    const registry = new CancellationWatchdogRegistry();
    const fetchRuns = vi.fn<() => Promise<RunState[]>>();
    registry.markPending("session-1", "turn-1");
    registry.arm("session-1", "turn-1", makeFactory({ current: true }, fetchRuns));

    registry.stop("session-1", "turn-1");

    expect(registry.watchdogKeys()).toEqual([]);
    expect(registry.pendingKeys()).toEqual([]);
    expect(registry.isPending("session-1", "turn-1")).toBe(false);
  });

  it("invokes the cleanup hook before the caller release when polling confirms terminal state", () => {
    const registry = new CancellationWatchdogRegistry();
    const fetchRuns = vi.fn<() => Promise<RunState[]>>();
    const cleanedUp: string[] = [];
    const factory: WatchdogFactory = (sessionId, turnId, onConfirmedTerminal) => {
      return {
        start: vi.fn(() => {
          onConfirmedTerminal();
          cleanedUp.push(`${sessionId}:${turnId}`);
        }),
        stop: vi.fn()
      };
    };

    registry.markPending("session-1", "turn-1");
    registry.arm("session-1", "turn-1", factory).start();

    expect(cleanedUp).toEqual(["session-1:turn-1"]);
    expect(registry.watchdogKeys()).toEqual([]);
    expect(registry.pendingKeys()).toEqual([]);
  });

  it("suspend keeps pending registration and resume restores a fresh watcher", () => {
    const registry = new CancellationWatchdogRegistry();
    const fetchRuns = vi.fn<() => Promise<RunState[]>>();
    const factory = makeFactory({ current: true }, fetchRuns);

    registry.markPending("session-1", "turn-1");
    registry.arm("session-1", "turn-1", factory);
    registry.suspendSession("session-1");

    // 暂停后：watcher 已移除，pending 登记保留
    expect(registry.watchdogKeys()).toEqual([]);
    expect(registry.pendingKeys()).toEqual(["session-1:turn-1"]);

    // 切回：恢复出一个全新 watcher
    const resumed = registry.resume("session-1", "turn-1", factory);
    expect(resumed).not.toBeNull();
    expect(registry.watchdogKeys()).toEqual(["session-1:turn-1"]);

    // 恢复不制造多个 watcher：再次 resume 返回同一实例
    expect(registry.resume("session-1", "turn-1", factory)).toBe(resumed);
    expect(registry.watchdogKeys()).toHaveLength(1);
  });

  it("does not resurrect terminal or stopped stale keys on resume", () => {
    const registry = new CancellationWatchdogRegistry();
    const fetchRuns = vi.fn<() => Promise<RunState[]>>();
    const factory = makeFactory({ current: true }, fetchRuns);

    // 情形 A：终态事件已到达（stop 清空登记）——resume 必须返回 null
    registry.markPending("session-1", "turn-1");
    registry.arm("session-1", "turn-1", factory);
    registry.stop("session-1", "turn-1");
    expect(registry.resume("session-1", "turn-1", factory)).toBeNull();

    // 情形 B：从未取消过的 turn——resume 返回 null
    expect(registry.resume("session-2", "turn-9", factory)).toBeNull();
  });

  it("stopSession removes every registration for the session including pending", () => {
    const registry = new CancellationWatchdogRegistry();
    const fetchRuns = vi.fn<() => Promise<RunState[]>>();
    registry.markPending("session-1", "turn-1");
    registry.arm("session-1", "turn-1", makeFactory({ current: true }, fetchRuns));
    registry.markPending("session-1", "turn-2");
    registry.arm("session-1", "turn-2", makeFactory({ current: true }, fetchRuns));
    registry.markPending("session-2", "turn-9");
    registry.arm("session-2", "turn-9", makeFactory({ current: true }, fetchRuns));

    registry.stopSession("session-1");

    expect(registry.watchdogKeys()).toEqual(["session-2:turn-9"]);
    expect(registry.pendingKeys()).toEqual(["session-2:turn-9"]);
  });

  it("stopSession clears pending keys left behind by suspendSession", () => {
    const registry = new CancellationWatchdogRegistry();
    const fetchRuns = vi.fn<() => Promise<RunState[]>>();
    // markPending -> arm -> suspendSession：watcher 已移除，pending 保留
    registry.markPending("session-1", "turn-1");
    registry.arm("session-1", "turn-1", makeFactory({ current: true }, fetchRuns));
    registry.suspendSession("session-1");
    expect(registry.watchdogKeys()).toEqual([]);
    expect(registry.pendingKeys()).toEqual(["session-1:turn-1"]);

    // 随后删除该非活动会话：stopSession 必须清理残留的 pending 登记
    registry.stopSession("session-1");
    expect(registry.watchdogKeys()).toEqual([]);
    expect(registry.pendingKeys()).toEqual([]);
    // 切回也不会复活
    expect(registry.resume("session-1", "turn-1", makeFactory({ current: true }, fetchRuns))).toBeNull();
  });

  it("stopSession after suspendSession keeps other sessions isolated", () => {
    const registry = new CancellationWatchdogRegistry();
    const fetchRuns = vi.fn<() => Promise<RunState[]>>();
    registry.markPending("session-1", "turn-1");
    registry.arm("session-1", "turn-1", makeFactory({ current: true }, fetchRuns));
    registry.suspendSession("session-1");
    registry.markPending("session-2", "turn-2");
    registry.arm("session-2", "turn-2", makeFactory({ current: true }, fetchRuns));

    registry.stopSession("session-1");

    // 会话 2 的登记不受影响
    expect(registry.watchdogKeys()).toEqual(["session-2:turn-2"]);
    expect(registry.pendingKeys()).toEqual(["session-2:turn-2"]);
    // 会话 1 即使原本处于 suspend 状态，其 pending 也已彻底清除
    expect(registry.pendingKeys()).not.toContain("session-1:turn-1");
  });

  it("leaves no residue across consecutive cancel cycles", () => {
    const registry = new CancellationWatchdogRegistry();
    const fetchRuns = vi.fn<() => Promise<RunState[]>>();
    const factory = makeFactory({ current: true }, fetchRuns);

    for (let index = 0; index < 3; index += 1) {
      const sessionId = `session-${index}`;
      const turnId = `turn-${index}`;
      registry.markPending(sessionId, turnId);
      registry.arm(sessionId, turnId, factory);
      // 轮询确认终态（经清理钩子）后登记应完全清空
      registry.stop(sessionId, turnId);
    }

    expect(registry.watchdogKeys()).toEqual([]);
    expect(registry.pendingKeys()).toEqual([]);
  });

  it("stopAll clears everything on unmount", () => {
    const registry = new CancellationWatchdogRegistry();
    const fetchRuns = vi.fn<() => Promise<RunState[]>>();
    registry.markPending("session-1", "turn-1");
    registry.arm("session-1", "turn-1", makeFactory({ current: true }, fetchRuns));
    registry.markPending("session-2", "turn-2");
    registry.arm("session-2", "turn-2", makeFactory({ current: true }, fetchRuns));

    registry.stopAll();

    expect(registry.watchdogKeys()).toEqual([]);
    expect(registry.pendingKeys()).toEqual([]);
  });
});
