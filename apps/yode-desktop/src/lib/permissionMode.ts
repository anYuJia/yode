import type { Bootstrap } from "./desktopTypes";
import type { PermissionModeSetRequest, PermissionModeState } from "./desktopIpc";

/** 完全信任只允许存在于当前应用进程，不能被持久化为用户默认值。 */
export const BYPASS_PERMISSION_SCOPE = "application-session" as const;

export function isBypassPermissionMode(mode: string) {
  return mode.trim().toLowerCase() === "bypass";
}

/**
 * 将前端请求与后端权限契约集中在一起，避免调用方遗漏 bypass 的确认或作用域。
 * 非 bypass 模式不应携带确认标志，后端会按用户默认权限持久化。
 */
export function permissionModeSetRequest(
  mode: string,
  bypassConfirmed = false
): PermissionModeSetRequest {
  if (isBypassPermissionMode(mode)) {
    return {
      mode,
      bypassConfirmed,
      scope: BYPASS_PERMISSION_SCOPE
    };
  }
  return { mode };
}

/**
 * 后端返回的 effectivePermissionMode 是唯一真相。不要用用户刚刚选择的 mode
 * 乐观更新 UI，否则 RPC 拒绝、后端降级或协议变化时会出现权限状态谎报。
 */
export function applyPermissionModeState(
  bootstrap: Bootstrap,
  state: PermissionModeState
): Bootstrap {
  return {
    ...bootstrap,
    permissionMode: state.effectivePermissionMode,
    effectivePermissionMode: state.effectivePermissionMode
  };
}
