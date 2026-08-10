import { RunStatus, RUN_STATUSES, TERMINAL_RUN_STATUSES } from "./desktopTypes";

/**
 * 运行状态机 reducer：turn 生命周期状态必须有明确有限集合与合法转移。
 * 状态域：null（无运行/新 turn 前）+ 九个持久化状态。
 *
 * 合法转移：
 * - null(idle) → starting → running
 * - running → waiting_approval / waiting_user / cancelling / completed / failed
 * - starting → running / waiting_approval / waiting_user / completed / failed / cancelled
 * - waiting_approval / waiting_user → running / cancelling / completed / failed / cancelled
 * - cancelling → cancelled / failed / interrupted
 * - 终态（completed / cancelled / failed / interrupted）不可被普通事件重新打开
 * - interrupted 不复活；新 turn 从 null(idle) 重新进入 starting
 */

/** 事件驱动的转移输入：事件 kind 或直接的后端状态。 */
export type RunTransition =
  | { type: "event"; kind: string }
  | { type: "status"; status: RunStatus };

const EVENT_KIND_TO_STATUS: Readonly<Record<string, RunStatus>> = {
  turn_started: "starting",
  assistant_text_delta: "running",
  assistant_reasoning_delta: "running",
  tool_started: "running",
  tool_progress: "running",
  retrying: "running",
  tool_confirm_required: "waiting_approval",
  plan_approval_required: "waiting_approval",
  ask_user: "waiting_user",
  cancelling: "cancelling",
  turn_completed: "completed",
  done: "completed",
  cancelled: "cancelled",
  error: "failed"
};

/** 把转移输入归一化为目标状态；无法映射的事件返回 null（不触发转移）。 */
export function runStatusForTransition(transition: RunTransition): RunStatus | null {
  if (transition.type === "status") return transition.status;
  return EVENT_KIND_TO_STATUS[transition.kind] ?? null;
}

/**
 * 状态机转移核心：
 * - 从终态状态出发（当前已终态且目标不同）→ 拒绝，保持原状态；
 * - cancelling 只能进入 cancelled / failed / interrupted；
 * - 其他转移按转移表放行；同一状态幂等。
 * 返回转移后的状态；null 表示无运行（idle）。
 */
export function transitionRunStatus(
  current: RunStatus | null,
  transition: RunTransition
): RunStatus | null {
  const target = runStatusForTransition(transition);
  if (target === null) return current;
  if (current === target) return current;

  // idle（无运行）下任何有效目标都可进入
  if (current === null) return target;

  if (TERMINAL_RUN_STATUSES.has(current)) {
    // 终态不可被普通事件重新打开（interrupted 也不允许复活）
    return current;
  }

  if (current === "cancelling") {
    if (target === "cancelled" || target === "failed" || target === "interrupted") {
      return target;
    }
    return current;
  }

  return target;
}

/** 事件 kind → 后端持久化状态（与 run_status_for_event_kind 语义一致）。 */
export function statusForEventKind(kind: string): RunStatus | null {
  return EVENT_KIND_TO_STATUS[kind] ?? null;
}
