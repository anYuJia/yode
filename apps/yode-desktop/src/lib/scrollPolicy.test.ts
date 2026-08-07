import { describe, expect, it } from "vitest";

import {
  distanceToBottom,
  isNearTimelineBottom,
  shouldAutoScrollOnUpdate,
  updateStickOnScroll,
  updateStickOnTouchMove,
  updateStickOnWheel
} from "./scrollPolicy";

function metrics(scrollTop: number, scrollHeight = 1000, clientHeight = 600): {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
} {
  return { scrollTop, scrollHeight, clientHeight };
}

describe("scroll follow policy", () => {
  it("computes distance to bottom", () => {
    expect(distanceToBottom(metrics(400))).toBe(0);
    expect(distanceToBottom(metrics(300))).toBe(100);
  });

  it("treats positions within the threshold as near bottom", () => {
    expect(isNearTimelineBottom(metrics(300, 1000, 600), 120)).toBe(true);
    expect(isNearTimelineBottom(metrics(100, 1000, 600), 120)).toBe(false);
  });

  it("keeps sticking when content grows and user is at the bottom", () => {
    expect(updateStickOnScroll(metrics(400), metrics(400), true)).toBe(true);
  });

  it("unsticks when the user scrolls up and never re-sticks until near bottom", () => {
    expect(updateStickOnScroll(metrics(400), metrics(200), true)).toBe(false);
    // 上滚后即使内容更新也不恢复跟随（距底 200px，超过 120px 阈值）
    expect(updateStickOnScroll(metrics(200), metrics(200), false)).toBe(false);
    // 回到接近底部时才恢复
    expect(updateStickOnScroll(metrics(200), metrics(300), false)).toBe(true);
  });

  it("wheel up unsticks, wheel down near bottom re-sticks", () => {
    expect(updateStickOnWheel(metrics(300), -10, true)).toBe(false);
    expect(updateStickOnWheel(metrics(300), 10, false)).toBe(true);
    expect(updateStickOnWheel(metrics(100), 10, false)).toBe(false);
  });

  it("touch move respects the same policy", () => {
    expect(updateStickOnTouchMove(metrics(300), 6, true)).toBe(false);
    expect(updateStickOnTouchMove(metrics(300), -6, false)).toBe(true);
  });

  it("never auto-scrolls when user scrolled away from the bottom", () => {
    // 用户停在中间位置（距底 300px > 80px 阈值）
    expect(shouldAutoScrollOnUpdate(metrics(100, 1000, 600), false)).toBe(false);
    // 虽然未跟随，但仍在阈值内 → 允许跟随
    expect(shouldAutoScrollOnUpdate(metrics(340, 1000, 600), false)).toBe(true);
    // 跟随状态下始终自动滚动
    expect(shouldAutoScrollOnUpdate(metrics(100, 1000, 600), true)).toBe(true);
  });
});
