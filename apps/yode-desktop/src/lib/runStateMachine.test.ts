import { describe, expect, it } from "vitest";

import {
  runStatusForTransition,
  statusForEventKind,
  transitionRunStatus
} from "./runStateMachine";

describe("run state machine", () => {
  it("follows the canonical lifecycle idle → starting → running → completed", () => {
    let status: ReturnType<typeof transitionRunStatus> = null;
    status = transitionRunStatus(status, { type: "event", kind: "turn_started" });
    expect(status).toBe("starting");
    status = transitionRunStatus(status, { type: "event", kind: "tool_started" });
    expect(status).toBe("running");
    status = transitionRunStatus(status, { type: "event", kind: "turn_completed" });
    expect(status).toBe("completed");
  });

  it("routes running to every waiting state and back", () => {
    let status = transitionRunStatus("running", { type: "event", kind: "tool_confirm_required" });
    expect(status).toBe("waiting_approval");
    status = transitionRunStatus(status, { type: "status", status: "running" });
    expect(status).toBe("running");
    status = transitionRunStatus(status, { type: "event", kind: "ask_user" });
    expect(status).toBe("waiting_user");
    status = transitionRunStatus(status, { type: "event", kind: "tool_started" });
    expect(status).toBe("running");
  });

  it("only lets cancelling reach cancelled/failed/interrupted", () => {
    expect(transitionRunStatus("cancelling", { type: "event", kind: "cancelled" })).toBe("cancelled");
    expect(transitionRunStatus("cancelling", { type: "event", kind: "error" })).toBe("failed");
    expect(transitionRunStatus("cancelling", { type: "status", status: "interrupted" })).toBe(
      "interrupted"
    );
    // 取消中不允许回到 running
    expect(transitionRunStatus("cancelling", { type: "event", kind: "tool_started" })).toBe(
      "cancelling"
    );
  });

  it("freezes terminal states against ordinary events", () => {
    for (const terminal of ["completed", "cancelled", "failed", "interrupted"] as const) {
      expect(transitionRunStatus(terminal, { type: "event", kind: "turn_started" })).toBe(terminal);
      expect(transitionRunStatus(terminal, { type: "status", status: "running" })).toBe(terminal);
      expect(transitionRunStatus(terminal, { type: "event", kind: "turn_completed" })).toBe(
        terminal
      );
    }
  });

  it("does not revive interrupted turns but allows a fresh turn to start", () => {
    expect(transitionRunStatus("interrupted", { type: "event", kind: "tool_started" })).toBe(
      "interrupted"
    );
    // 新 turn：从 idle（null）重新进入 starting
    expect(transitionRunStatus(null, { type: "event", kind: "turn_started" })).toBe("starting");
  });

  it("keeps unknown event kinds from changing state", () => {
    expect(transitionRunStatus("running", { type: "event", kind: "cost_update" })).toBe("running");
    expect(transitionRunStatus(null, { type: "event", kind: "unknown_kind" })).toBeNull();
  });

  it("is idempotent for same-state transitions", () => {
    expect(transitionRunStatus("waiting_user", { type: "event", kind: "ask_user" })).toBe(
      "waiting_user"
    );
  });

  it("maps event kinds to backend statuses consistently", () => {
    expect(statusForEventKind("turn_started")).toBe("starting");
    expect(statusForEventKind("tool_confirm_required")).toBe("waiting_approval");
    expect(statusForEventKind("ask_user")).toBe("waiting_user");
    expect(statusForEventKind("cancelling")).toBe("cancelling");
    expect(statusForEventKind("done")).toBe("completed");
    expect(statusForEventKind("cancelled")).toBe("cancelled");
    expect(statusForEventKind("error")).toBe("failed");
    expect(statusForEventKind("usage_update")).toBeNull();
  });

  it("normalizes status transitions from backend state", () => {
    expect(runStatusForTransition({ type: "status", status: "waiting_approval" })).toBe(
      "waiting_approval"
    );
    expect(runStatusForTransition({ type: "event", kind: "nope" })).toBeNull();
  });
});
