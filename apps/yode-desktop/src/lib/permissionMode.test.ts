import { describe, expect, it } from "vitest";

import { fallbackBootstrap } from "./desktopTypes";
import {
  BYPASS_PERMISSION_SCOPE,
  applyPermissionModeState,
  isBypassPermissionMode,
  permissionModeSetRequest
} from "./permissionMode";

describe("permission mode request contract", () => {
  it("requires the explicit application-session contract for bypass", () => {
    expect(permissionModeSetRequest("bypass", true)).toEqual({
      mode: "bypass",
      bypassConfirmed: true,
      scope: BYPASS_PERMISSION_SCOPE
    });
    expect(permissionModeSetRequest("bypass", false)).toEqual({
      mode: "bypass",
      bypassConfirmed: false,
      scope: BYPASS_PERMISSION_SCOPE
    });
  });

  it("does not attach a bypass confirmation to persistent modes", () => {
    expect(permissionModeSetRequest("auto")).toEqual({ mode: "auto" });
    expect(isBypassPermissionMode(" Bypass ")).toBe(true);
    expect(isBypassPermissionMode("auto")).toBe(false);
  });

  it("uses the backend effective mode instead of the requested mode", () => {
    const next = applyPermissionModeState(fallbackBootstrap, {
      effectivePermissionMode: "default",
      scope: "user-default",
      persisted: true,
      bypassActive: false
    });

    expect(next.permissionMode).toBe("default");
    expect(next.effectivePermissionMode).toBe("default");
  });
});
