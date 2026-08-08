import { applyDesktopEventToTimelineItems } from "./timelineUtils";
import { isUserQuery } from "./askUser";
import type { UserQuery } from "./askUser";
import {
  DesktopEvent,
  PendingUserQuestion,
  RunState,
  TimelineItem,
  UsageSnapshot
} from "./desktopTypes";
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
  getCurrentTurnId?: (sessionId: string) => string | null;
  sendSystemNotification: (title: string, body: string, policy: NotificationPolicy) => void;
  setCurrentTurnId: (turnId: string | null, sessionId?: string | null) => void;
  setIsProcessing: (isProcessing: boolean, sessionId?: string | null) => void;
  setPendingUserQuestion: (question: PendingUserQuestionUpdater, sessionId?: string | null) => void;
  setTimelineItems: (updater: (items: TimelineItem[]) => TimelineItem[], sessionId?: string | null) => void;
  setUsageSnapshot: (
    updater: (current: UsageSnapshot | null) => UsageSnapshot | null,
    sessionId?: string | null
  ) => void;
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
function acceptSequencedEvent(
  sessionId: string,
  turnId: string,
  seq: number,
  options: { allowCancelled?: boolean } = {}
): boolean {
  const track = getTurnTrack(sessionId, turnId);
  if ((track.cancelled && !options.allowCancelled) || track.done) return false;
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
// flush 时读取最新处理器上下文。每个 delta 都携带会话 ID，写入其专属状态，
// 因此切换会话不会让 25ms 窗口内的旧 delta 污染当前时间线。
let latestHandlerContext: DesktopEventHandlerContext | null = null;
// 事件驱动的当前 turn。不同会话可以同时运行，必须独立追踪。
const lastSeenTurns = new Map<string, string>();

/** 取消后的状态核验延迟。到期时只查询后端状态，绝不基于时间直接解锁 UI。 */
export const CANCELLATION_STATUS_WATCHDOG_MS = 2500;

export type CancellationWatchdogDecision = "ignore" | "wait" | "release";

const TERMINAL_TURN_EVENT_KINDS = new Set(["cancelled", "done", "turn_completed", "error"]);
const TERMINAL_RUN_STATUSES = new Set(["cancelled", "completed", "done", "failed"]);

export function isCurrentTurnId(currentTurnId: string | null | undefined, turnId: string | undefined) {
  return currentTurnId !== null && currentTurnId !== undefined && currentTurnId === turnId;
}

/**
 * 判断事件是否为某个 turn 的后端终态事件（cancelled/done/turn_completed/error）。
 * 返回匹配的 (sessionId, turnId)；非终态事件或无法识别归属时返回 null。
 * 调用方可用它在终态事件到达时立即停止该 turn 的取消轮询 watchdog。
 */
export function isTerminalTurnEvent(
  payload: unknown,
  eventKind?: string
): { sessionId: string; turnId: string } | null {
  const envelope = desktopEventEnvelope(payload, eventKind);
  if (!envelope.sessionId || !envelope.turnId) return null;
  return TERMINAL_TURN_EVENT_KINDS.has(envelope.kind)
    ? { sessionId: envelope.sessionId, turnId: envelope.turnId }
    : null;
}

/** 只有对应当前 turn 的后端终态事件才能将会话恢复为可发送状态。 */
export function shouldReleaseCurrentTurnForTerminalEvent(
  currentTurnId: string | null | undefined,
  turnId: string | undefined,
  kind: string
) {
  return TERMINAL_TURN_EVENT_KINDS.has(kind) && isCurrentTurnId(currentTurnId, turnId);
}

/**
 * watchdog 只能依据同一 session + turn 的后端终态解锁。
 * 若用户已经开始新 turn，或后端仍未报告终态，则保持当前状态不变。
 */
export function cancellationWatchdogDecision({
  currentTurnId,
  sessionId,
  turnId,
  runs
}: {
  currentTurnId: string | null | undefined;
  sessionId: string;
  turnId: string;
  runs: RunState[];
}): CancellationWatchdogDecision {
  if (!isCurrentTurnId(currentTurnId, turnId)) return "ignore";
  const run = runs.find((candidate) => candidate.sessionId === sessionId && candidate.turnId === turnId);
  return run && TERMINAL_RUN_STATUSES.has(run.status) ? "release" : "wait";
}

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
  // 会话 + turn 双重隔离：同一会话内切到新 turn 后，旧 turn 已入队的
  // delta 一并丢弃；其他会话的 delta 更新各自快照，不会写入当前界面。
  const accepted = batch.filter(
    (envelope) =>
      envelope.sessionId != null &&
      envelope.turnId != null &&
      envelope.turnId === lastSeenTurns.get(envelope.sessionId)
  );
  if (accepted.length === 0) return;
  const groupedBySession = new Map<string, DesktopEventEnvelope[]>();
  for (const envelope of accepted) {
    const sessionEvents = groupedBySession.get(envelope.sessionId!) ?? [];
    sessionEvents.push(envelope);
    groupedBySession.set(envelope.sessionId!, sessionEvents);
  }
  // 同一会话的多个增量合并在一次状态更新中应用，保持顺序并减少 React 提交。
  for (const [sessionId, sessionEvents] of groupedBySession) {
    context.setTimelineItems(
      (items) => {
        let next = items;
        for (const envelope of sessionEvents) {
          next = applyDesktopEventToTimelineItems(
            next,
            envelope.desktopEvent ?? envelope.rawPayload,
            envelope.desktopEvent ? undefined : envelope.kind
          );
        }
        return next;
      },
      sessionId
    );
  }
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
  const targetSessionId = envelope.sessionId ?? context.activeSessionId;

  // 完整的 DesktopEvent 信封：按 turn 隔离并过滤重复/乱序/取消后迟到事件
  if (envelope.desktopEvent && envelope.sessionId && envelope.turnId) {
    const { seq, kind } = envelope.desktopEvent;
    // currentTurnId 由会话专属快照提供；只有该会话当前 turn 的事件能更新快照。
    const currentTurnId = context.getCurrentTurnId?.(envelope.sessionId) ?? context.currentTurnId;
    const ownsCurrentTurn = isCurrentTurnId(currentTurnId, envelope.turnId);
    const isCurrentTurn =
      ownsCurrentTurn || lastSeenTurns.get(envelope.sessionId) === envelope.turnId;

    // 非当前 turn 的常规事件直接丢弃，避免跨 turn 串入同一会话。
    // 生命周期事件（cancelling/cancelled）仍需流转以维护该 turn 的轨道状态。
    if (
      !isCurrentTurn &&
      kind !== "turn_started" &&
      kind !== "cancelling" &&
      kind !== "cancelled"
    ) {
      return;
    }

    // 事件驱动地维护“当前 turn”（在 seq 门禁之后更新）：
    // - turn_started：已有进行中的 turn 时，同会话其他 turn 的 turn_started
    //   视为过期/乱序事件拒绝（不覆盖当前 turn）。
    // - 终态事件（turn_completed/error/cancelled/done）清空当前 turn。
    if (kind === "turn_started") {
      if (
        lastSeenTurns.has(envelope.sessionId) &&
        lastSeenTurns.get(envelope.sessionId) !== envelope.turnId
      ) {
        return;
      }
      if (!acceptSequencedEvent(envelope.sessionId, envelope.turnId, seq)) {
        return;
      }
      lastSeenTurns.set(envelope.sessionId, envelope.turnId);
    } else if (kind === "cancelled") {
      if (!acceptSequencedEvent(envelope.sessionId, envelope.turnId, seq, { allowCancelled: true })) {
        return;
      }
      if (lastSeenTurns.get(envelope.sessionId) === envelope.turnId) {
        lastSeenTurns.delete(envelope.sessionId);
      }
    } else {
      if (!acceptSequencedEvent(envelope.sessionId, envelope.turnId, seq)) {
        return;
      }
      if (
        (kind === "turn_completed" || kind === "error" || kind === "done") &&
        lastSeenTurns.get(envelope.sessionId) === envelope.turnId
      ) {
        lastSeenTurns.delete(envelope.sessionId);
      }
    }

    if (kind === "cancelling" || kind === "cancelled") {
      flushBatch();
      const track = getTurnTrack(envelope.sessionId, envelope.turnId);
      track.cancelled = true;
      if (kind === "cancelled") {
        if (shouldReleaseCurrentTurnForTerminalEvent(currentTurnId, envelope.turnId, kind)) {
          context.setIsProcessing(false, envelope.sessionId);
          context.setCurrentTurnId(null, envelope.sessionId);
        }
        context.setPendingUserQuestion(
          (current) => (current && current.turnId === envelope.turnId ? null : current),
          envelope.sessionId
        );
        // 更新取消提示为终态
        context.setTimelineItems(
          (items) =>
            items.map((item) =>
              item.kind === "boundary" && item.id.includes(`cancel-${envelope.turnId}-`)
                ? { ...item, title: "已手动终止", body: "用户已取消此轮运行。" }
                : item
            ),
          envelope.sessionId
        );
      }
      return;
    }

    if (kind === "done") {
      flushBatch();
      const track = getTurnTrack(envelope.sessionId, envelope.turnId);
      track.done = true;
      if (shouldReleaseCurrentTurnForTerminalEvent(currentTurnId, envelope.turnId, kind)) {
        context.setIsProcessing(false, envelope.sessionId);
        context.setCurrentTurnId(null, envelope.sessionId);
      }
      return;
    }
  }

  if (envelope.kind === "turn_started") {
    flushBatch();
    context.setIsProcessing(true, targetSessionId);
    if (envelope.turnId) {
      context.setCurrentTurnId(envelope.turnId, targetSessionId);
    }
  } else if (envelope.kind === "ask_user" && envelope.sessionId && envelope.turnId) {
    flushBatch();
    context.sendSystemNotification(
      "Yode 需要你的回复",
      stringField(envelope.payloadRecord, "body", "任务正在等待输入。"),
      "question"
    );
    context.setPendingUserQuestion(
      {
        sessionId: envelope.sessionId,
        turnId: envelope.turnId,
        title: optionalStringField(envelope.payloadRecord, "title"),
        question: stringField(envelope.payloadRecord, "body", "请回复问题"),
        query: userQueryField(envelope.payloadRecord, "query")
      },
      targetSessionId
    );
  } else if (envelope.kind === "tool_confirm_required" || envelope.kind === "permission") {
    flushBatch();
    context.sendSystemNotification(
      "Yode 请求执行权限",
      stringField(envelope.payloadRecord, "body", "有操作需要确认。"),
      "permission"
    );
  } else if (envelope.kind === "usage_update" || envelope.kind === "cost_update") {
    context.setUsageSnapshot(
      (current) => mergeUsageSnapshot(current, envelope.payloadRecord),
      targetSessionId
    );
  } else if (envelope.kind === "turn_completed" || envelope.kind === "error") {
    flushBatch();
    const currentTurnId = envelope.sessionId
      ? (context.getCurrentTurnId?.(envelope.sessionId) ?? context.currentTurnId)
      : context.currentTurnId;
    const shouldReleaseCurrentTurn = shouldReleaseCurrentTurnForTerminalEvent(
      currentTurnId,
      envelope.turnId,
      envelope.kind
    );
    if (shouldReleaseCurrentTurn) {
      context.setIsProcessing(false, targetSessionId);
      context.setCurrentTurnId(null, targetSessionId);
      context.setPendingUserQuestion(null, targetSessionId);
    }
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
  context.setTimelineItems(
    (items) =>
      applyDesktopEventToTimelineItems(
        items,
        envelope.desktopEvent ?? envelope.rawPayload,
        envelope.desktopEvent ? undefined : envelope.kind
      ),
    targetSessionId
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
  lastSeenTurns.clear();
}

/**
 * 丢弃未 flush 的增量批缓冲。
 * 指定会话时仅丢弃该会话的批缓冲；无参数仅供全局重置场景使用。
 */
export function discardPendingDeltas(sessionId?: string) {
  if (batchTimer !== null) {
    clearTimeout(batchTimer);
    batchTimer = null;
  }
  if (sessionId === undefined) {
    pendingBatch = [];
    lastSeenTurns.clear();
    return;
  }
  pendingBatch = pendingBatch.filter((envelope) => envelope.sessionId !== sessionId);
  lastSeenTurns.delete(sessionId);
}

/**
 * 清空指定会话的全部 turn 轨道，例如会话被移除后不再接收其迟到事件。
 */
export function resetTurnTracksForSession(sessionId: string) {
  const prefix = `${sessionId}:`;
  for (const key of turnTracks.keys()) {
    if (key.startsWith(prefix)) {
      turnTracks.delete(key);
    }
  }
  lastSeenTurns.delete(sessionId);
  pendingBatch = pendingBatch.filter((envelope) => envelope.sessionId !== sessionId);
}
