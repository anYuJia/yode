// 时间线滚动跟随策略（纯函数，便于单元测试）。

export type TimelineMetrics = {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
};

export function distanceToBottom(metrics: TimelineMetrics): number {
  return metrics.scrollHeight - metrics.scrollTop - metrics.clientHeight;
}

export function isNearTimelineBottom(metrics: TimelineMetrics, threshold = 120): boolean {
  return distanceToBottom(metrics) < threshold;
}

/**
 * 用户滚动后更新跟随状态：
 * - 向上滚动（远离底部）→ 解除跟随，禁止后续自动抢滚；
 * - 回到接近底部 → 恢复跟随。
 */
export function updateStickOnScroll(
  previous: TimelineMetrics,
  next: TimelineMetrics,
  wasSticking: boolean
): boolean {
  const scrolledUp = next.scrollTop < previous.scrollTop - 1;
  if (scrolledUp) return false;
  if (isNearTimelineBottom(next, 120)) return true;
  return wasSticking;
}

/** 滚轮事件更新跟随状态（deltaY < 0 为上滚）。 */
export function updateStickOnWheel(
  current: TimelineMetrics,
  deltaY: number,
  wasSticking: boolean
): boolean {
  if (deltaY < 0) return false;
  if (deltaY > 0 && isNearTimelineBottom(current, 160)) return true;
  return wasSticking;
}

/** 触屏手势：上滑远离底部时解除跟随，下滑接近底部时恢复。 */
export function updateStickOnTouchMove(
  current: TimelineMetrics,
  touchDeltaY: number,
  wasSticking: boolean
): boolean {
  if (touchDeltaY > 2) return false;
  if (touchDeltaY < -2 && isNearTimelineBottom(current, 160)) return true;
  return wasSticking;
}

/**
 * 内容更新后是否自动滚动到底部：只有仍处于跟随状态、
 * 或当前已接近底部时才滚动，用户主动上滚后绝不抢滚。
 */
export function shouldAutoScrollOnUpdate(
  current: TimelineMetrics,
  wasSticking: boolean,
  threshold = 80
): boolean {
  if (!wasSticking && !isNearTimelineBottom(current, threshold)) return false;
  return true;
}
