import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { permissionKeyAllowed, submitPermissionDecision } from "./permissionActions";

// node 测试环境没有 DOM Node，这里提供一个最小可用实现
class FakeNode {
  parent: FakeNode | null;
  constructor(parent: FakeNode | null = null) {
    this.parent = parent;
  }
  contains(target: unknown) {
    let current = target as FakeNode | null;
    while (current) {
      if (current === this) return true;
      current = current.parent;
    }
    return false;
  }
}

const OriginalNode = (globalThis as { Node?: unknown }).Node;

beforeEach(() => {
  (globalThis as { Node?: unknown }).Node = FakeNode;
});

afterEach(() => {
  if (OriginalNode === undefined) {
    delete (globalThis as { Node?: unknown }).Node;
  } else {
    (globalThis as { Node?: unknown }).Node = OriginalNode;
  }
});

// 测试中把 FakeNode 断言为浏览器 Node/EventTarget
const asNode = (node: FakeNode) => node as unknown as Node;
const asTarget = (node: FakeNode) => node as unknown as EventTarget;

describe("permission keyboard boundary", () => {
  it("does not respond to Enter pressed outside the permission panel", () => {
    const panel = asNode(new FakeNode());
    const input = asTarget(new FakeNode());
    expect(permissionKeyAllowed(input, panel)).toBe(false);
    expect(permissionKeyAllowed(null, panel)).toBe(false);
  });

  it("responds to keys pressed inside the permission panel", () => {
    const panel = new FakeNode();
    const optionButton = new FakeNode(panel);
    expect(permissionKeyAllowed(asTarget(optionButton), asNode(panel))).toBe(true);
    expect(permissionKeyAllowed(asTarget(panel), asNode(panel))).toBe(true);
  });

  it("does not respond when the panel is not mounted", () => {
    expect(permissionKeyAllowed(asTarget(new FakeNode()), null)).toBe(false);
  });
});

describe("permission decision submission", () => {
  it("only removes the card after the RPC succeeds", async () => {
    const submit = vi.fn().mockResolvedValue(undefined);

    const result = await submitPermissionDecision({
      sessionId: "s1",
      turnId: "t1",
      decision: "allow_once",
      submit
    });

    expect(result).toBe("ok");
  });

  it("keeps the card when the RPC fails", async () => {
    const submit = vi.fn().mockRejectedValue(new Error("backend unavailable"));

    const result = await submitPermissionDecision({
      sessionId: "s1",
      turnId: "t1",
      decision: "always_allow",
      submit
    });

    expect(result).toBe("failed");
    // 可重试：同一决策再次提交
    submit.mockResolvedValue(undefined);
    const retry = await submitPermissionDecision({
      sessionId: "s1",
      turnId: "t1",
      decision: "always_allow",
      submit
    });
    expect(retry).toBe("ok");
  });

  it("rejects when session or turn info is missing", async () => {
    const submit = vi.fn();
    const result = await submitPermissionDecision({
      decision: "deny",
      submit
    });
    expect(result).toBe("missing-info");
    expect(submit).not.toHaveBeenCalled();
  });

  it("maps decisions to backend arguments correctly", async () => {
    const submit = vi.fn().mockResolvedValue(undefined);
    await submitPermissionDecision({
      sessionId: "s1",
      turnId: "t1",
      decision: "always_allow",
      submit
    });
    expect(submit).toHaveBeenCalledWith({
      sessionId: "s1",
      turnId: "t1",
      allow: true,
      alwaysAllow: true
    });

    await submitPermissionDecision({
      sessionId: "s1",
      turnId: "t1",
      decision: "deny",
      submit
    });
    expect(submit).toHaveBeenLastCalledWith({
      sessionId: "s1",
      turnId: "t1",
      allow: false,
      alwaysAllow: false
    });
  });
});
