import { applyDesktopEventToTimelineItems } from "./timelineUtils";
import { isUserQuery } from "./askUser";
import type { UserQuery } from "./askUser";
import { DesktopEvent, PendingUserQuestion, TimelineItem, UsageSnapshot } from "./desktopTypes";
import { recordFromUnknown } from "./jsonUtils";

type NotificationPolicy = "completion" | "permission" | "question";

type PendingUserQuestionUpdater =
  | PendingUserQuestion
  | null
  | ((current: PendingUserQuestion | null) => PendingUserQuestion | null);

type DesktopEventHandlerContext = {
  activeSessionId: string | null;
  eventKind?: string;
  payload: unknown;
  currentTurnId?: string | null;
  sendSystemNotification: (title: string, body: string, policy: NotificationPolicy) => void;
  setCurrentTurnId: (turnId: string | null) => void;
  setIsProcessing: (isProcessing: boolean) => void;
  setPendingUserQuestion: (question: PendingUserQuestionUpdater) => void;
  setTimelineItems: (updater: (items: TimelineItem[]) => TimelineItem[]) => void;
  setUsageSnapshot: (updater: (current: UsageSnapshot | null) => UsageSnapshot | null) => void;
};

type DesktopEventEnvelope = {
  desktopEvent?: DesktopEvent;
  kind: string;
  payloadRecord: Record<string, unknown>;
  rawPayload: unknown;
  sessionId?: string;
  turnId?: string;
};

/** 单个 turn 的事件流状态：seq 单调跟踪 + 终态标记。 */
type TurnTrack = {
  lastSeq: number;
  cancelled: boolean;
  done: boolean;
};

// 每个 (sessionId, turnId) 的独立事件轨道，用于丢弃重复、乱序、取消后迟到事件。
const turnTracks = new Map<string, TurnTrack>();
const MAX_TRACKS = 64;

function turnTrackKey(sessionId: string, turnId: string) {
  return `${sessionId}:${turnId}`;
}

function getTurnTrack(sessionId: string, turnId: string): TurnTrack {
  const key = turnTrackKey(sessionId, turnId);
  let track = turnTracks.get(key);
  if (!track) {
    if (turnTracks.size >= MAX_TRACKS) {
      const oldest = turnTracks.keys().next().value;
      if (oldest !== undefined) turnTracks.delete(oldest);
    }
    track = { lastSeq: -1, cancelled: false, done: false };
    turnTracks.set(key, track);
  }
  return track;
}

/** 丢弃重复、乱序、已取消/已完成 turn 的迟到事件。返回 false 表示应丢弃。 */
function acceptSequencedEvent(sessionId: string, turnId: string, seq: number): boolean {
  const track = getTurnTrack(sessionId, turnId);
  if (track.cancelled || track.done) return false;
  if (seq <= track.lastSeq) return false;
  track.lastSeq = seq;
  return true;
}

// ─── 流式 delta 合帧 ────────────────────────────────────────────────────────
// 高频的文本/推理增量事件在 25ms 内合并为一次状态更新，
// 限制 UI 提交频率（约 40 次/秒），同时不损失事件顺序。
const DELTA_BATCH_MS = 25;
let pendingBatch: DesktopEventEnvelope[] = [];
let batchTimer: number | null = null;
// flush 时读取“最新”的处理器上下文（含最新 activeSessionId），
// 避免 25ms 窗口内切换会话后旧 delta 写入新会话时间线。
let latestHandlerContext: DesktopEventHandlerContext | null = null;
// 事件驱动的“当前 turn”：由 turn_started 置位、终态事件清空，
// 比 React state（异步同步）更及时，用于合帧按 turn 过滤。
let lastSeenTurn: { sessionId: string; turnId: string } | null = null;

function flushBatch() {
  if (batchTimer !== null) {
    clearTimeout(batchTimer);
    batchTimer = null;
  }
  if (pendingBatch.length === 0) return;
  const batch = pendingBatch;
  pendingBatch = [];
  const context = latestHandlerContext;
  if (!context) return;
  // 会话 + turn 双重隔离：只应用属于当前激活会话、且属于当前 turn 的增量。
  // 同一会话内切到新 turn 后，旧 turn 已入队的 delta 一并丢弃。
  const activeSession = context.activeSessionId;
  const accepted = batch.filter(
    (envelope) =>
      (!activeSession || !envelope.sessionId || envelope.sessionId === activeSession) &&
      envelope.turnId != null &&
      lastSeenTurn != null &&
      envelope.turnId === lastSeenTurn.turnId
  );
  if (accepted.length === 0) return;
  if (accepted.length === 1) {
    const envelope = accepted[0];
    context.setTimelineItems((items) =>
      applyDesktopEventToTimelineItems(
        items,
        envelope.desktopEvent ?? envelope.rawPayload,
        envelope.desktopEvent ? undefined : envelope.kind
      )
    );
    return;
  }
  // 多个增量合并在一次状态更新中应用，保持顺序并减少 React 提交
  context.setTimelineItems((items) => {
    let next = items;
    for (const envelope of accepted) {
      next = applyDesktopEventToTimelineItems(
        next,
        envelope.desktopEvent ?? envelope.rawPayload,
        envelope.desktopEvent ? undefined : envelope.kind
      );
    }
    return next;
  });
}

function isDeltaKind(kind: string) {
  return kind === "assistant_text_delta" || kind === "assistant_reasoning_delta";
}

function scheduleBatchFlush() {
  if (batchTimer !== null) return;
  batchTimer = setTimeout(() => {
    batchTimer = null;
    flushBatch();
  }, DELTA_BATCH_MS);
}

export function handleDesktopRuntimeEvent(context: DesktopEventHandlerContext) {
  latestHandlerContext = context;
  const envelope = desktopEventEnvelope(context.payload, context.eventKind);
  if (
    envelope.sessionId &&
    context.activeSessionId &&
    envelope.sessionId !== context.activeSessionId
  ) {
    return;
  }

  // 完整的 DesktopEvent 信封：按 turn 隔离并过滤重复/乱序/取消后迟到事件
  if (envelope.desktopEvent && envelope.sessionId && envelope.turnId) {
    const { seq, kind } = envelope.desktopEvent;
    // currentTurnId 为 null（turn 刚结束或会话刚切换）时，
    // 只接受轨道中已存在且未终态的 turn（如 turn_completed 后的 done），
    // 杜绝旧会话/旧 turn 的迟到事件混入新时间线。
    const isCurrentTurn =
      context.currentTurnId == null
        ? turnTracks.has(turnTrackKey(envelope.sessionId, envelope.turnId))
        : envelope.turnId === context.currentTurnId;

    // 非当前 turn 的常规事件直接丢弃，避免跨 turn/跨会话串入。
    // 生命周期事件（cancelling/cancelled）仍需流转以维护该 turn 的轨道状态。
    if (
      !isCurrentTurn &&
      kind !== "turn_started" &&
      kind !== "cancelling" &&
      kind !== "cancelled"
    ) {
      return;
    }

    // 事件驱动地维护“当前 turn”（在门禁之后更新）：
    // - turn_started：已有进行中的 turn 时，同会话其他 turn 的 turn_started
    //   视为过期/乱序事件拒绝（不覆盖当前 turn）；跨会话的 turn_started
    //   由 App 层 discardPendingDeltas 重置后才会被接受。
    // - 终态事件（turn_completed/error/cancelling/cancelled/done）清空当前 turn。
    if (kind === "turn_started") {
      if (
        lastSeenTurn != null &&
        (lastSeenTurn.sessionId !== envelope.sessionId ||
          lastSeenTurn.turnId !== envelope.turnId)
      ) {
        return;
      }
      lastSeenTurn = { sessionId: envelope.sessionId, turnId: envelope.turnId };
    } else if (
      kind === "turn_completed" ||
      kind === "error" ||
      kind === "cancelling" ||
      kind === "cancelled" ||
      kind === "done"
    ) {
      if (lastSeenTurn != null && lastSeenTurn.turnId === envelope.turnId) {
        lastSeenTurn = null;
      }
    }

    if (kind === "cancelling" || kind === "cancelled") {
      flushBatch();
      const track = getTurnTrack(envelope.sessionId, envelope.turnId);
      track.cancelled = true;
      if (kind === "cancelled") {
        if (isCurrentTurn) {
          context.setIsProcessing(false);
          context.setCurrentTurnId(null);
        }
        context.setPendingUserQuestion((current) =>
          current && current.turnId === envelope.turnId ? null : current
        );
        // 更新取消提示为终态
        context.setTimelineItems((items) =>
          items.map((item) =>
            item.kind === "boundary" && item.id.includes(`cancel-${envelope.turnId}-`)
              ? { ...item, title: "已手动终止", body: "用户已取消此轮运行。" }
              : item
          )
        );
      }
      return;
    }

    if (!acceptSequencedEvent(envelope.sessionId, envelope.turnId, seq)) {
      return;
    }

    if (kind === "done") {
      flushBatch();
      const track = getTurnTrack(envelope.sessionId, envelope.turnId);
      track.done = true;
      if (isCurrentTurn) {
        context.setIsProcessing(false);
        context.setCurrentTurnId(null);
      }
      return;
    }
  }

  if (envelope.kind === "turn_started") {
    flushBatch();
    context.setIsProcessing(true);
    if (envelope.turnId) {
      context.setCurrentTurnId(envelope.turnId);
    }
  } else if (envelope.kind === "ask_user" && envelope.sessionId && envelope.turnId) {
    flushBatch();
    context.sendSystemNotification(
      "Yode 需要你的回复",
      stringField(envelope.payloadRecord, "body", "任务正在等待输入。"),
      "question"
    );
    context.setPendingUserQuestion({
      sessionId: envelope.sessionId,
      turnId: envelope.turnId,
      title: optionalStringField(envelope.payloadRecord, "title"),
      question: stringField(envelope.payloadRecord, "body", "请回复问题"),
      query: userQueryField(envelope.payloadRecord, "query")
    });
  } else if (envelope.kind === "tool_confirm_required" || envelope.kind === "permission") {
    flushBatch();
    context.sendSystemNotification(
      "Yode 请求执行权限",
      stringField(envelope.payloadRecord, "body", "有操作需要确认。"),
      "permission"
    );
  } else if (envelope.kind === "usage_update" || envelope.kind === "cost_update") {
    context.setUsageSnapshot((current) => mergeUsageSnapshot(current, envelope.payloadRecord));
  } else if (envelope.kind === "turn_completed" || envelope.kind === "error") {
    flushBatch();
    const isCurrentTurn =
      context.currentTurnId == null ||
      (envelope.turnId ? envelope.turnId === context.currentTurnId : true);
    if (isCurrentTurn) {
      context.setIsProcessing(false);
      context.setCurrentTurnId(null);
    }
    context.setPendingUserQuestion(null);
    if (envelope.kind === "turn_completed") {
      context.sendSystemNotification(
        "Yode 已完成任务",
        stringField(envelope.payloadRecord, "body", "本轮运行已完成。").slice(0, 160),
        "completion"
      );
    }
  }

  if (isDeltaKind(envelope.kind)) {
    // 高频增量事件合帧，降低 React 提交频率
    pendingBatch.push(envelope);
    scheduleBatchFlush();
    return;
  }

  flushBatch();
  context.setTimelineItems((items) =>
    applyDesktopEventToTimelineItems(
      items,
      envelope.desktopEvent ?? envelope.rawPayload,
      envelope.desktopEvent ? undefined : envelope.kind
    )
  );
}

function desktopEventEnvelope(payload: unknown, eventKind?: string): DesktopEventEnvelope {
  const raw = recordFromUnknown(payload) ?? {};
  const desktopEvent = isDesktopEvent(raw) ? raw : undefined;
  const nestedPayload = recordFromUnknown(desktopEvent?.payload ?? raw.payload) ?? {};
  const kind = desktopEvent?.kind ?? eventKind ?? stringField(raw, "kind", "");
  return {
    desktopEvent,
    kind,
    payloadRecord: nestedPayload,
    rawPayload: payload,
    sessionId: desktopEvent?.sessionId ?? optionalStringField(raw, "sessionId"),
    turnId: desktopEvent?.turnId ?? optionalStringField(raw, "turnId")
  };
}

function mergeUsageSnapshot(
  current: UsageSnapshot | null,
  payload: Record<string, unknown>
): UsageSnapshot {
  const inputTokens = numberField(payload, "inputTokens") ?? current?.inputTokens;
  const outputTokens = numberField(payload, "outputTokens") ?? current?.outputTokens;
  return {
    ...current,
    estimatedCost: numberField(payload, "estimatedCost") ?? current?.estimatedCost,
    inputTokens,
    outputTokens,
    totalTokens:
      numberField(payload, "totalTokens") ??
      (inputTokens !== undefined || outputTokens !== undefined
        ? (inputTokens ?? 0) + (outputTokens ?? 0)
        : current?.totalTokens),
    cacheWriteTokens: numberField(payload, "cacheWriteTokens") ?? current?.cacheWriteTokens,
    cacheReadTokens: numberField(payload, "cacheReadTokens") ?? current?.cacheReadTokens
  };
}

function isDesktopEvent(value: Record<string, unknown> | undefined): value is DesktopEvent {
  return Boolean(
    value &&
      typeof value.sessionId === "string" &&
      typeof value.turnId === "string" &&
      typeof value.seq === "number" &&
      typeof value.kind === "string" &&
      typeof value.timestamp === "string" &&
      recordFromUnknown(value.payload)
  );
}

function userQueryField(value: Record<string, unknown>, key: string): UserQuery | undefined {
  const raw = value[key];
  return isUserQuery(raw) ? raw : undefined;
}

function optionalStringField(value: Record<string, unknown> | undefined, key: string) {
  const raw = value?.[key];
  return typeof raw === "string" ? raw : undefined;
}

function stringField(value: Record<string, unknown> | undefined, key: string, fallback: string) {
  const raw = value?.[key];
  return typeof raw === "string" ? raw : fallback;
}

function numberField(value: Record<string, unknown>, key: string) {
  const raw = value[key];
  return typeof raw === "number" ? raw : undefined;
}

/** 测试辅助：清空合帧缓冲与 turn 轨道。 */
export function resetDesktopEventFiltersForTest() {
  if (batchTimer !== null) {
    clearTimeout(batchTimer);
    batchTimer = null;
  }
  pendingBatch = [];
  turnTracks.clear();
  latestHandlerContext = null;
  lastSeenTurn = null;
}

/**
 * 丢弃未 flush 的增量批缓冲。
 * 切换会话/新建会话时调用，防止 25ms 窗口内的旧会话 delta 写入新会话时间线。
 */
export function discardPendingDeltas() {
  if (batchTimer !== null) {
    clearTimeout(batchTimer);
    batchTimer = null;
  }
  pendingBatch = [];
  lastSeenTurn = null;
}

/**
 * 清空指定会话的全部 turn 轨道。
 * 切换会话时调用：此后该会话的迟到事件因“无轨道记录”而被丢弃。
 */
export function resetTurnTracksForSession(sessionId: string) {
  const prefix = `${sessionId}:`;
  for (const key of turnTracks.keys()) {
    if (key.startsWith(prefix)) {
      turnTracks.delete(key);
    }
  }
}
