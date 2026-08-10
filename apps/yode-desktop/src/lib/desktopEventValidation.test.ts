import { describe, expect, it } from "vitest";

import {
  validateDesktopEventEnvelope,
  validateRunState,
  validateTurnEventPayload
} from "./desktopEventValidation";

function envelope(overrides: Record<string, unknown> = {}) {
  return {
    schemaVersion: 1,
    sessionId: "session-1",
    turnId: "turn-1",
    seq: 3,
    timestamp: "2026-08-08T00:00:00Z",
    kind: "tool_started",
    payload: { id: "call-1", tool: "bash", title: "工具", body: "cmd", status: "running" },
    ...overrides
  };
}

describe("desktop event envelope validation", () => {
  it("accepts a well-formed envelope and returns typed fields", () => {
    const result = validateDesktopEventEnvelope(envelope());
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.sessionId).toBe("session-1");
      expect(result.value.turnId).toBe("turn-1");
      expect(result.value.seq).toBe(3);
      expect(result.value.kind).toBe("tool_started");
      expect(result.value.schemaVersion).toBe(1);
    }
  });

  it("accepts the legacy shape without schemaVersion (backward compatible)", () => {
    const { schemaVersion: _dropped, ...legacy } = envelope();
    const result = validateDesktopEventEnvelope(legacy);
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.value.schemaVersion).toBeUndefined();
  });

  it("allows unknown payload fields for forward extension", () => {
    const result = validateDesktopEventEnvelope(
      envelope({ payload: { id: "x", tool: "bash", futureField: { nested: [1] } } })
    );
    expect(result.ok).toBe(true);
  });

  it("rejects an unknown kind without crashing (kept for diagnostics)", () => {
    const result = validateDesktopEventEnvelope(envelope({ kind: "future_kind_xyz" }));
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toContain("future_kind_xyz");
  });

  it("rejects wrong field types", () => {
    expect(validateDesktopEventEnvelope(envelope({ seq: "3" })).ok).toBe(false);
    expect(validateDesktopEventEnvelope(envelope({ sessionId: 42 })).ok).toBe(false);
    expect(validateDesktopEventEnvelope(envelope({ payload: "not-an-object" })).ok).toBe(false);
    expect(validateDesktopEventEnvelope(envelope({ timestamp: 123 })).ok).toBe(false);
    expect(validateDesktopEventEnvelope("plain string").ok).toBe(false);
  });

  it("rejects non-finite numbers", () => {
    expect(validateDesktopEventEnvelope(envelope({ seq: NaN })).ok).toBe(false);
  });
});

describe("run state validation", () => {
  it("accepts every status in the finite set", () => {
    for (const status of [
      "starting",
      "running",
      "waiting_approval",
      "waiting_user",
      "cancelling",
      "completed",
      "cancelled",
      "failed",
      "interrupted"
    ]) {
      const result = validateRunState({
        sessionId: "s",
        turnId: "t",
        status,
        updatedAt: "2026-08-08T00:00:00Z"
      });
      expect(result.ok, status).toBe(true);
    }
  });

  it("rejects unknown status strings instead of accepting them silently", () => {
    const result = validateRunState({
      sessionId: "s",
      turnId: "t",
      status: "streaming",
      updatedAt: "2026-08-08T00:00:00Z"
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toContain("streaming");
  });

  it("accepts optional journal fields and validates their types", () => {
    const ok = validateRunState({
      sessionId: "s",
      turnId: "t",
      status: "interrupted",
      updatedAt: "2026-08-08T00:00:00Z",
      startedAt: "2026-08-08T00:00:00Z",
      endedAt: "2026-08-08T00:00:00Z",
      lastSeq: 12,
      errorCode: "interrupted_on_startup",
      detail: "检测到上次运行未正常结束",
      cancellationRequested: true
    });
    expect(ok.ok).toBe(true);
    if (ok.ok) {
      expect(ok.value.lastSeq).toBe(12);
      expect(ok.value.errorCode).toBe("interrupted_on_startup");
      expect(ok.value.cancellationRequested).toBe(true);
    }
    expect(
      validateRunState({
        sessionId: "s",
        turnId: "t",
        status: "running",
        updatedAt: "2026-08-08T00:00:00Z",
        lastSeq: "high"
      }).ok
    ).toBe(false);
  });
});

describe("turn event payload validation", () => {
  it("accepts unknown fields while rejecting wrong types for known keys", () => {
    expect(validateTurnEventPayload({ id: "x", body: "ok", custom: [1, 2] }).ok).toBe(true);
    expect(validateTurnEventPayload({ body: 42 }).ok).toBe(false);
    expect(validateTurnEventPayload({ inputTokens: "10" }).ok).toBe(false);
    expect(validateTurnEventPayload({ query: "not a query" }).ok).toBe(false);
    expect(validateTurnEventPayload(null).ok).toBe(false);
  });

  it("accepts a well-formed user query structure", () => {
    const result = validateTurnEventPayload({
      query: {
        questions: [{ header: "h", question: "q", options: [] }]
      }
    });
    expect(result.ok).toBe(true);
  });
});
