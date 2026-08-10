import type { RunState, TurnEventRecord } from "./desktopTypes";
import { TERMINAL_RUN_STATUSES } from "./desktopTypes";
import { validateDesktopEventEnvelope } from "./desktopEventValidation";
import { turnEventsSince } from "./desktopIpc";

/**
 * 断线恢复与事件重放。
 *
 * 重放只针对未终态 turn（starting/running/waiting_approval 等），按
 * sessionId + turnId + lastSeq 从持久化 journal 读取事件并重新注入同一事件
 * 管道。管道内的 turn 轨道（turnTracks）按 (sessionId, turnId, seq) 去重，
 * 保证：
 * - 重放幂等（重复调用不会重复渲染）；
 * - 乱序事件不能覆盖较新的状态（seq 门禁）；
 * - 已终态 turn 的迟到事件不会污染当前时间线（终态轨道冻结）。
 */

export type ReplayOutcome = {
  ok: boolean;
  /** 已重放的 turn 数。 */
  replayedTurns: number;
  /** 已重放事件数。 */
  replayedEvents: number;
  /** 失败信息：查询失败时保留当前锁定状态，由调用方提供可重试路径。 */
  error?: string;
  /** 诊断信息：未知 kind / 异常事件被安全保留，不影响重放结果。 */
  warnings?: string[];
};

export type ReplayDispatcher = (payload: unknown) => void;

export function shouldReplayRun(run: RunState): boolean {
  return !TERMINAL_RUN_STATUSES.has(run.status);
}

/** 重放单个 turn：读取 seq > sinceSeq 的事件并注入事件管道。 */
export async function replayTurnEvents(options: {
  runs: RunState[];
  dispatch: ReplayDispatcher;
}): Promise<ReplayOutcome> {
  const outcome: ReplayOutcome = { ok: true, replayedTurns: 0, replayedEvents: 0 };
  const failures: string[] = [];
  const warnings: string[] = [];

  for (const run of options.runs) {
    if (!shouldReplayRun(run)) continue;
    try {
      const sinceSeq = typeof run.lastSeq === "number" ? run.lastSeq : -1;
      const events = await turnEventsSince(run.sessionId, run.turnId, sinceSeq, null);
      outcome.replayedTurns += 1;
      for (const event of events) {
        outcome.replayedEvents += 1;
        dispatchReplayEvent(event, options.dispatch, failures, warnings);
      }
    } catch (err) {
      failures.push(
        `重放会话 ${run.sessionId} turn ${run.turnId} 失败: ${String(err)}`
      );
    }
  }

  if (warnings.length > 0) {
    outcome.warnings = warnings;
  }
  if (failures.length > 0) {
    outcome.ok = false;
    outcome.error = failures.join("；");
  }
  return outcome;
}

/** 把持久化事件还原为桌面事件信封注入事件管道。
 * 未知 kind 安全保留到诊断（warnings），不得导致前端崩溃，也不中断重放。 */
function dispatchReplayEvent(
  event: TurnEventRecord,
  dispatch: ReplayDispatcher,
  failures: string[],
  warnings: string[]
): void {
  const envelope = {
    sessionId: event.sessionId,
    turnId: event.turnId,
    seq: event.seq,
    timestamp: event.timestamp,
    kind: event.kind,
    payload: event.payload
  };
  const validated = validateDesktopEventEnvelope(envelope);
  if (!validated.ok) {
    // 未知 kind / 异常事件：保留到诊断日志，不回放
    warnings.push(`重放事件 kind=${event.kind} seq=${event.seq} 被拒绝: ${validated.error}`);
    return;
  }
  dispatch(envelope);
}
