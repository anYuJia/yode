// 权限确认的决策提交逻辑（纯函数，便于单元测试）。

export type PermissionDecision = "allow_once" | "always_allow" | "deny";

export type PermissionSubmitArgs = {
  sessionId: string;
  turnId: string;
  allow: boolean;
  alwaysAllow: boolean;
};

/**
 * 提交权限决定：只有后端 RPC 成功才返回 "ok"。
 * 调用方（权限卡片）仅在 "ok" 时移除卡片，RPC 失败时保留卡片以便重试。
 */
export async function submitPermissionDecision(opts: {
  sessionId?: string;
  turnId?: string;
  decision: PermissionDecision;
  submit: (args: PermissionSubmitArgs) => Promise<void>;
}): Promise<"ok" | "missing-info" | "failed"> {
  if (!opts.sessionId || !opts.turnId) return "missing-info";
  try {
    await opts.submit({
      sessionId: opts.sessionId,
      turnId: opts.turnId,
      allow: opts.decision !== "deny",
      alwaysAllow: opts.decision === "always_allow"
    });
    return "ok";
  } catch {
    return "failed";
  }
}

/**
 * 键盘事件是否应触发权限面板操作：
 * 焦点必须位于确认面板内部，普通输入框、终端或其他控件的 Enter/Esc
 * 一律不触发批准或拒绝。
 */
export function permissionKeyAllowed(target: EventTarget | null, panel: Node | null): boolean {
  if (!panel) return false;
  if (typeof Node === "undefined") return false;
  return target instanceof Node && panel.contains(target);
}
