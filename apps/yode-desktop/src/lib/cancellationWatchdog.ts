import {
  CANCELLATION_STATUS_WATCHDOG_MS,
  cancellationWatchdogDecision
} from "./desktopEventHandlers";
import type { RunState } from "./desktopTypes";

/**
 * 取消后状态核验轮询的退避间隔：attempt 从 1 开始，1s → 2s → 4s → 8s → 封顶 15s。
 * 绝不使用固定超时直接解锁，只是以受控节奏持续查询后端终态。
 */
export function nextCancellationPollDelayMs(attempt: number): number {
  return Math.min(1000 * 2 ** Math.min(Math.max(attempt, 1) - 1, 4), 15000);
}

export type CancellationWatchdogOptions = {
  sessionId: string;
  turnId: string;
  /**
   * 双重隔离守卫：必须返回“该会话的当前 turn 仍是被取消的 turn”。
   * 为 false 时（用户已开始新 turn、切换会话后状态被清理等）立即停止轮询，
   * 绝不让旧 watchdog 解锁新任务。
   */
  isStillCurrent: () => boolean;
  fetchRuns: () => Promise<RunState[]>;
  /** 后端确认终态：允许调用方解锁 UI（仅当 isStillCurrent 仍为 true 时触发）。 */
  onReleased: () => void;
  /** 后端仍报告 running/cancelling 或运行记录缺失：继续退避重试。 */
  onWaiting: () => void;
  /** 查询失败：不放弃，继续退避重试。 */
  onQueryError: () => void;
  schedule: (callback: () => void, delayMs: number) => number;
  clear: (handle: number) => void;
};

export type CancellationWatchdog = {
  start: () => void;
  stop: () => void;
};

/**
 * 取消运行后的状态对账 watchdog。
 *
 * 首次核验等待 [`CANCELLATION_STATUS_WATCHDOG_MS`] 后进行 runs_list 查询；
 * 只要后端仍报告 running/cancelling、运行记录缺失或查询失败，就按
 * [`nextCancellationPollDelayMs`] 退避重试，直到同一 (sessionId, turnId)
 * 出现后端终态为止。终态事件由调用方通过 `stop()` 主动终止轮询。
 */
export function startCancellationWatchdog(
  options: CancellationWatchdogOptions
): CancellationWatchdog {
  let handle: number | null = null;
  let stopped = false;
  let started = false;
  let attempt = 0;

  const clearPending = () => {
    if (handle !== null) {
      options.clear(handle);
      handle = null;
    }
  };

  const stop = () => {
    stopped = true;
    started = true;
    clearPending();
  };

  const scheduleNext = () => {
    if (stopped) return;
    attempt += 1;
    handle = options.schedule(() => {
      handle = null;
      poll();
    }, nextCancellationPollDelayMs(attempt));
  };

  const poll = () => {
    if (stopped) return;
    if (!options.isStillCurrent()) {
      stop();
      return;
    }
    void options
      .fetchRuns()
      .then((runs) => {
        // 外部 stop() 与请求返回竞态：停止后不得调用任何回调。
        if (stopped) return;
        if (!options.isStillCurrent()) {
          stop();
          return;
        }
        const decision = cancellationWatchdogDecision({
          currentTurnId: options.turnId,
          sessionId: options.sessionId,
          turnId: options.turnId,
          runs
        });
        switch (decision) {
          case "release":
            stop();
            options.onReleased();
            return;
          case "wait":
            options.onWaiting();
            scheduleNext();
            return;
          case "ignore":
            // 不应发生（isStillCurrent 已隔离），防御性停止。
            stop();
        }
      })
      .catch(() => {
        if (stopped) return;
        if (!options.isStillCurrent()) {
          stop();
          return;
        }
        options.onQueryError();
        scheduleNext();
      });
  };

  return {
    start: () => {
      // 幂等：重复 start（双击取消、重复武装）不得产生第二个初始计时器
      // 或重复的 runs_list 请求；停止后同样不可重启。
      if (stopped || started) return;
      started = true;
      attempt = 0;
      handle = options.schedule(() => {
        handle = null;
        poll();
      }, CANCELLATION_STATUS_WATCHDOG_MS);
    },
    stop
  };
}
