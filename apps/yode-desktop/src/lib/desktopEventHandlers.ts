import { applyDesktopEventToTimelineItems } from "./timelineUtils";
import { isUserQuery } from "./askUser";
import type { UserQuery } from "./askUser";
import { DesktopEvent, PendingUserQuestion, TimelineItem, UsageSnapshot } from "./desktopTypes";

type NotificationPolicy = "completion" | "permission" | "question";

type DesktopEventHandlerContext = {
  activeSessionId: string | null;
  eventKind?: string;
  payload: unknown;
  sendSystemNotification: (title: string, body: string, policy: NotificationPolicy) => void;
  setCurrentTurnId: (turnId: string) => void;
  setIsProcessing: (isProcessing: boolean) => void;
  setPendingUserQuestion: (question: PendingUserQuestion | null) => void;
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

export function handleDesktopRuntimeEvent(context: DesktopEventHandlerContext) {
  const envelope = desktopEventEnvelope(context.payload, context.eventKind);
  if (
    envelope.sessionId &&
    context.activeSessionId &&
    envelope.sessionId !== context.activeSessionId
  ) {
    return;
  }

  if (envelope.kind === "turn_started") {
    context.setIsProcessing(true);
    if (envelope.turnId) {
      context.setCurrentTurnId(envelope.turnId);
    }
  } else if (envelope.kind === "ask_user" && envelope.sessionId && envelope.turnId) {
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
    context.sendSystemNotification(
      "Yode 请求执行权限",
      stringField(envelope.payloadRecord, "body", "有操作需要确认。"),
      "permission"
    );
  } else if (envelope.kind === "usage_update" || envelope.kind === "cost_update") {
    context.setUsageSnapshot((current) => mergeUsageSnapshot(current, envelope.payloadRecord));
  } else if (envelope.kind === "turn_completed" || envelope.kind === "error") {
    context.setIsProcessing(false);
    context.setPendingUserQuestion(null);
    if (envelope.kind === "turn_completed") {
      context.sendSystemNotification(
        "Yode 已完成任务",
        stringField(envelope.payloadRecord, "body", "本轮运行已完成。").slice(0, 160),
        "completion"
      );
    }
  }

  context.setTimelineItems((items) =>
    applyDesktopEventToTimelineItems(
      items,
      envelope.desktopEvent ?? envelope.rawPayload,
      envelope.desktopEvent ? undefined : envelope.kind
    )
  );
}

function desktopEventEnvelope(payload: unknown, eventKind?: string): DesktopEventEnvelope {
  const raw = objectRecord(payload) ?? {};
  const desktopEvent = isDesktopEvent(raw) ? (raw as DesktopEvent) : undefined;
  const nestedPayload = objectRecord(desktopEvent?.payload ?? raw.payload) ?? {};
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
  return Boolean(value && typeof value.kind === "string" && typeof value.payload === "object");
}

function objectRecord(value: unknown): Record<string, unknown> | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  return value as Record<string, unknown>;
}

function objectField(value: Record<string, unknown>, key: string): Record<string, unknown> | undefined {
  return objectRecord(value[key]);
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
