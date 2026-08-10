import {
  DESKTOP_EVENT_KINDS,
  RUN_STATUSES,
  type DesktopEventEnvelope,
  type DesktopEventKind,
  type RunState,
  type RunStatus,
  type TurnEventPayload
} from "./desktopTypes";
import { recordFromUnknown } from "./jsonUtils";
import { isUserQuery } from "./askUser";

/**
 * 运行时校验函数：对未知字段允许扩展，对错误类型拒绝。
 * 返回 null 表示校验通过（返回类型化对象）；失败返回诊断字符串。
 */

export type ValidationResult<T> = { ok: true; value: T } | { ok: false; error: string };

function fail(error: string): ValidationResult<never> {
  return { ok: false, error };
}

function stringField(value: Record<string, unknown>, key: string): string | null {
  const raw = value[key];
  return typeof raw === "string" ? raw : null;
}

function numberField(value: Record<string, unknown>, key: string): number | null {
  const raw = value[key];
  return typeof raw === "number" && Number.isFinite(raw) ? raw : null;
}

function optionalString(value: Record<string, unknown>, key: string): string | null | undefined {
  const raw = value[key];
  if (raw === undefined || raw === null) return undefined;
  return typeof raw === "string" ? raw : null;
}

/** 校验统一事件信封：schemaVersion 可选（旧版事件无此字段），
 * 关键字段类型错误一律拒绝；未知 kind 保留到诊断（返回失败但带原值）。 */
export function validateDesktopEventEnvelope(value: unknown): ValidationResult<DesktopEventEnvelope> {
  const raw = recordFromUnknown(value);
  if (!raw) return fail("事件不是 JSON 对象");

  const sessionId = stringField(raw, "sessionId");
  const turnId = stringField(raw, "turnId");
  const seq = numberField(raw, "seq");
  const timestamp = stringField(raw, "timestamp");
  const kind = stringField(raw, "kind");
  if (sessionId === null) return fail("事件缺少 sessionId（字符串）");
  if (turnId === null) return fail("事件缺少 turnId（字符串）");
  if (seq === null) return fail("事件缺少 seq（数字）");
  if (timestamp === null) return fail("事件缺少 timestamp（字符串）");
  if (kind === null) return fail("事件缺少 kind（字符串）");

  if (raw.schemaVersion !== undefined && typeof raw.schemaVersion !== "number") {
    return fail("schemaVersion 必须为数字");
  }

  const payload = recordFromUnknown(raw.payload);
  if (!payload) return fail("事件 payload 必须是 JSON 对象");

  // 未知 kind：拒绝进入强类型管道（由调用方保留到诊断日志）
  if (!(DESKTOP_EVENT_KINDS as readonly string[]).includes(kind)) {
    return fail(`未知事件 kind: ${kind}`);
  }

  return {
    ok: true,
    value: {
      schemaVersion: typeof raw.schemaVersion === "number" ? raw.schemaVersion : undefined,
      sessionId,
      turnId,
      seq,
      timestamp,
      kind: kind as DesktopEventKind,
      payload: payload as unknown as TurnEventPayload
    }
  };
}

/** 校验 RunState：status 必须是有限状态集合成员。 */
export function validateRunState(value: unknown): ValidationResult<RunState> {
  const raw = recordFromUnknown(value);
  if (!raw) return fail("run 不是 JSON 对象");
  const sessionId = stringField(raw, "sessionId");
  const turnId = stringField(raw, "turnId");
  const status = stringField(raw, "status");
  const updatedAt = stringField(raw, "updatedAt");
  if (sessionId === null) return fail("run 缺少 sessionId");
  if (turnId === null) return fail("run 缺少 turnId");
  if (status === null) return fail("run 缺少 status");
  if (updatedAt === null) return fail("run 缺少 updatedAt");
  if (!(RUN_STATUSES as readonly string[]).includes(status)) {
    return fail(`未知 run 状态: ${status}`);
  }
  const lastSeq = numberField(raw, "lastSeq");
  if (raw.lastSeq !== undefined && lastSeq === null) return fail("lastSeq 必须为数字");
  const detail = optionalString(raw, "detail");
  if (detail === null) return fail("detail 类型错误");
  const errorCode = optionalString(raw, "errorCode");
  if (errorCode === null) return fail("errorCode 类型错误");
  const startedAt = optionalString(raw, "startedAt");
  if (startedAt === null) return fail("startedAt 类型错误");
  const endedAt = optionalString(raw, "endedAt");
  if (endedAt === null) return fail("endedAt 类型错误");

  return {
    ok: true,
    value: {
      sessionId,
      turnId,
      status: status as RunStatus,
      updatedAt,
      detail,
      errorCode,
      startedAt,
      endedAt,
      lastSeq: lastSeq ?? undefined,
      cancellationRequested:
        typeof raw.cancellationRequested === "boolean"
          ? raw.cancellationRequested
          : undefined
    }
  };
}

/** 校验 turn 事件 payload：允许未知字段，拒绝关键字段类型错误。 */
export function validateTurnEventPayload(value: unknown): ValidationResult<TurnEventPayload> {
  const raw = recordFromUnknown(value);
  if (!raw) return fail("payload 不是 JSON 对象");
  for (const key of ["id", "tool", "title", "body", "status", "meta", "reasoning"]) {
    if (raw[key] !== undefined && typeof raw[key] !== "string") {
      return fail(`payload 字段 ${key} 必须为字符串`);
    }
  }
  for (const key of [
    "inputTokens",
    "outputTokens",
    "totalTokens",
    "cacheWriteTokens",
    "cacheReadTokens",
    "attempt",
    "maxAttempts",
    "delaySecs",
    "percent",
    "removed",
    "toolResultsTruncated"
  ]) {
    if (raw[key] !== undefined && (typeof raw[key] !== "number" || !Number.isFinite(raw[key]))) {
      return fail(`payload 字段 ${key} 必须为数字`);
    }
  }
  if (raw.query !== undefined && !isUserQuery(raw.query)) {
    return fail("payload 字段 query 必须是合法用户问题结构");
  }
  return { ok: true, value: raw as unknown as TurnEventPayload };
}

export function validationErrorMessage(result: { ok: false; error: string }): string {
  return result.error;
}
