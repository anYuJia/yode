import { afterEach, describe, expect, it, vi } from "vitest";

import { handleDesktopRuntimeEvent, resetDesktopEventFiltersForTest } from "./desktopEventHandlers";
import { messagesToTimelineItems, mergeOlderTimelineItems } from "./timelineUtils";
import type { DesktopMessage, TimelineItem, UsageSnapshot } from "./desktopTypes";

/**
 * 长会话性能基准：
 * - 事件管道：1,000 / 5,000 / 10,000 条事件（含 25ms delta 合帧）的
 *   处理耗时、UI 提交次数与时间线增长（内存增长的近似代理）；
 * - 消息分页：1,000 / 5,000 / 10,000 条消息的窗口前置合并耗时。
 * 只记录数据并做宽松上限断言（防回归），不依赖具体机器性能。
 */

const BENCH_SCALE = [1000, 5000, 10000] as const;

function benchEvent(sessionId: string, turnId: string, seq: number, kind: string, body: string) {
  return {
    sessionId,
    turnId,
    seq,
    kind,
    timestamp: new Date().toISOString(),
    payload: { body, id: `call-${seq}`, tool: "bash", title: "t", status: "running" }
  };
}

function makePipeline(sessionId: string, turnId: string) {
  let timeline: TimelineItem[] = [];
  let commits = 0;
  let usage: UsageSnapshot | null = null;
  const context = {
    activeSessionId: sessionId,
    currentTurnId: turnId,
    getCurrentTurnId: (sid: string) => (sid === sessionId ? turnId : null),
    payload: {},
    sendSystemNotification: vi.fn(),
    setCurrentTurnId: vi.fn(),
    setIsProcessing: vi.fn(),
    setPendingUserQuestion: vi.fn(),
    setTimelineItems: vi.fn((updater: (items: TimelineItem[]) => TimelineItem[]) => {
      timeline = updater(timeline);
      commits += 1;
    }),
    setUsageSnapshot: vi.fn((updater: (current: UsageSnapshot | null) => UsageSnapshot | null) => {
      usage = updater(usage);
    })
  };
  return {
    dispatch: (payload: unknown) => handleDesktopRuntimeEvent({ ...context, payload }),
    getTimeline: () => timeline,
    getCommits: () => commits
  };
}

describe("长会话事件管道性能基准", () => {
  afterEach(() => {
    resetDesktopEventFiltersForTest();
  });

  for (const count of BENCH_SCALE) {
    it(`处理 ${count.toLocaleString()} 条流式事件（1 次 turn_started + 增量 + 终态）`, () => {
      const sessionId = "bench-session";
      const turnId = "bench-turn";
      const pipeline = makePipeline(sessionId, turnId);
      const started = performance.now();
      // 事件序列：turn_started → 连续文本增量（25ms 合帧）+ 周期性工具事件 → turn_completed
      pipeline.dispatch(benchEvent(sessionId, turnId, 0, "turn_started", "思考中"));
      for (let index = 1; index < count - 1; index += 1) {
        // 每 10 个增量插入一个工具事件（触发合帧 flush），模拟真实流式节奏
        const kind = index % 10 === 0 ? "tool_started" : "assistant_text_delta";
        pipeline.dispatch(
          benchEvent(sessionId, turnId, index, kind, `事件内容片段 ${index}`)
        );
      }
      pipeline.dispatch(benchEvent(sessionId, turnId, count - 1, "turn_completed", "完成"));
      const elapsed = performance.now() - started;
      const timelineLength = pipeline.getTimeline().length;
      const commits = pipeline.getCommits();
      // eslint-disable-next-line no-console
      console.log(
        `[perf] events=${count} elapsed=${elapsed.toFixed(1)}ms commits=${commits} timelineItems=${timelineLength}`
      );
      // 时间线必须包含全部事件信息，且 25ms 合帧显著降低 UI 提交次数（远小于事件数）
      expect(timelineLength).toBeGreaterThan(0);
      expect(commits).toBeLessThanOrEqual(count);
      // 每个工具事件触发一次合帧 flush + 一次工具提交；合帧后提交数约为事件数的 1/5
      expect(commits).toBeLessThanOrEqual(Math.ceil(count / 5) + 4);
      expect(elapsed).toBeLessThan(30_000);
    });
  }
});

describe("长会话消息分页性能基准", () => {
  for (const count of BENCH_SCALE) {
    it(`窗口合并 ${count.toLocaleString()} 条历史消息`, () => {
      const messages: DesktopMessage[] = [];
      for (let index = 0; index < count; index += 1) {
        messages.push({
          id: index,
          sortOrder: index,
          role: index % 2 === 0 ? "user" : "assistant",
          content: `历史消息 ${index}`,
          createdAt: new Date(2026, 0, 1, 0, 0, index % 60).toISOString()
        });
      }
      // 首次窗口（最近 100 条，后端降序 → 前端反转升序）
      const firstWindow = messagesToTimelineItems([...messages.slice(-100)].reverse());
      let timeline = [...firstWindow];
      const started = performance.now();
      // 向上翻页：逐窗口前置合并，直到加载完整个历史
      let cursor = messages.length - 100;
      let windows = 0;
      while (cursor > 0) {
        const start = Math.max(0, cursor - 100);
        const older = messagesToTimelineItems([...messages.slice(start, cursor)].reverse());
        timeline = mergeOlderTimelineItems(timeline, older);
        cursor = start;
        windows += 1;
      }
      const elapsed = performance.now() - started;
      // 消息顺序不丢失：时间线数量等于全部消息项
      expect(timeline.length).toBeGreaterThanOrEqual(count);
      // 首次窗口 + 向上翻页窗口合计 = 全部消息
      expect(windows + 1).toBe(Math.ceil(count / 100));
      // eslint-disable-next-line no-console
      console.log(
        `[perf] messages=${count} windows=${windows} elapsed=${elapsed.toFixed(1)}ms timelineItems=${timeline.length}`
      );
      expect(elapsed).toBeLessThan(30_000);
    });
  }

  it("前置合并幂等：重复窗口不产生重复项", () => {
    const messages: DesktopMessage[] = [];
    for (let index = 0; index < 100; index += 1) {
      messages.push({ id: index, role: "user", content: `m${index}`, createdAt: "2026-01-01T00:00:00Z" });
    }
    const timeline = messagesToTimelineItems([...messages].reverse());
    const merged = mergeOlderTimelineItems(timeline, timeline);
    expect(merged).toBe(timeline);
  });
});
