import type { CancellationWatchdog } from "./cancellationWatchdog";

/**
 * 构造 watcher 的工厂。registry 会注入 `onConfirmedTerminal` 钩子：
 * 轮询确认终态时先由 registry 清理该 (sessionId, turnId) 的登记，
 * 再执行调用方的 UI 解锁逻辑。
 */
export type WatchdogFactory = (
  sessionId: string,
  turnId: string,
  onConfirmedTerminal: () => void
) => CancellationWatchdog;

/**
 * 取消轮询 watchdog 的登记簿：按 (sessionId, turnId) 维护活跃 watcher 与
 * “已取消但未确认终态”的 pending 登记，保证：
 * - 同一 turn 最多只有一个 watcher（幂等武装，双击取消/重复恢复不产生重复轮询）；
 * - 终态（后端事件或轮询确认）后同时清理 watcher 与 pending 登记；
 * - 切换会话暂停定时器但保留 pending 登记，切回后凭登记恢复；
 * - 已经终态或已经停止的陈旧 key 不会被恢复。
 */
export class CancellationWatchdogRegistry {
  private watchdogs = new Map<string, CancellationWatchdog>();
  private pendingCancellations = new Set<string>();

  private key(sessionId: string, turnId: string): string {
    return `${sessionId}:${turnId}`;
  }

  /**
   * 统一的会话 key 枚举：同时覆盖 watchdogs 与 pendingCancellations 两套集合，
   * 保证针对会话的清理操作永远同时作用于两者，不会因“只遍历 watchdogs”漏掉
   * suspend 后仅剩 pending 登记的陈旧 key。
   */
  private keysForSession(sessionId: string): string[] {
    const prefix = `${sessionId}:`;
    const keys = new Set<string>();
    for (const key of this.watchdogs.keys()) {
      if (key.startsWith(prefix)) keys.add(key);
    }
    for (const key of this.pendingCancellations.keys()) {
      if (key.startsWith(prefix)) keys.add(key);
    }
    return [...keys];
  }

  /** 登记“已发起取消、尚未收到后端终态”的 turn。 */
  markPending(sessionId: string, turnId: string): void {
    this.pendingCancellations.add(this.key(sessionId, turnId));
  }

  isPending(sessionId: string, turnId: string): boolean {
    return this.pendingCancellations.has(this.key(sessionId, turnId));
  }

  has(sessionId: string, turnId: string): boolean {
    return this.watchdogs.has(this.key(sessionId, turnId));
  }

  /**
   * 幂等武装：已登记则返回现有 watcher（不重复创建）。
   * 通过 factory 注入终态清理钩子，轮询确认终态后自动清除登记。
   */
  arm(sessionId: string, turnId: string, factory: WatchdogFactory): CancellationWatchdog {
    const key = this.key(sessionId, turnId);
    const existing = this.watchdogs.get(key);
    if (existing) return existing;
    const watchdog = factory(sessionId, turnId, () => this.stop(sessionId, turnId));
    this.watchdogs.set(key, watchdog);
    return watchdog;
  }

  /** 终态（后端事件到达或轮询确认）：停止 watcher 并清除 watcher 与 pending 登记。 */
  stop(sessionId: string, turnId: string): void {
    const key = this.key(sessionId, turnId);
    this.watchdogs.get(key)?.stop();
    this.watchdogs.delete(key);
    this.pendingCancellations.delete(key);
  }

  /**
   * 新 turn 接管或删除会话：终止该会话全部 watcher 并清除该会话在
   * watchdogs 与 pendingCancellations 两个集合中的全部登记（含
   * suspend 后仅剩 pending 登记的陈旧 key）。
   */
  stopSession(sessionId: string): void {
    for (const key of this.keysForSession(sessionId)) {
      this.watchdogs.get(key)?.stop();
      this.watchdogs.delete(key);
      this.pendingCancellations.delete(key);
    }
  }

  /** 切换会话：暂停该会话定时器（保留 pending 登记），切回后凭登记恢复。 */
  suspendSession(sessionId: string): void {
    for (const key of this.keysForSession(sessionId)) {
      this.watchdogs.get(key)?.stop();
      this.watchdogs.delete(key);
    }
  }

  /**
   * 切回会话后恢复对账：仅当仍有 pending 登记且无现成 watcher 时重新武装。
   * 已终态（stop 已清理登记）或已停止的陈旧 key 不会复活。
   */
  resume(
    sessionId: string,
    turnId: string,
    factory: WatchdogFactory
  ): CancellationWatchdog | null {
    const key = this.key(sessionId, turnId);
    if (!this.pendingCancellations.has(key)) return null;
    const existing = this.watchdogs.get(key);
    if (existing) return existing;
    const watchdog = factory(sessionId, turnId, () => this.stop(sessionId, turnId));
    this.watchdogs.set(key, watchdog);
    return watchdog;
  }

  /** 组件卸载：停止全部 watcher 并清空全部登记。 */
  stopAll(): void {
    for (const watchdog of this.watchdogs.values()) {
      watchdog.stop();
    }
    this.watchdogs.clear();
    this.pendingCancellations.clear();
  }

  /** 测试辅助：当前活跃 watcher 的 key 集合。 */
  watchdogKeys(): string[] {
    return [...this.watchdogs.keys()];
  }

  /** 测试辅助：当前 pending 登记的 key 集合。 */
  pendingKeys(): string[] {
    return [...this.pendingCancellations];
  }
}
