import { describe, expect, it, vi } from "vitest";

import { invoke } from "@tauri-apps/api/core";
import { permissionModeSet } from "./desktopIpc";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn()
}));

const invokeMock = vi.mocked(invoke);

describe("permission_mode_set IPC", () => {
  it("forwards the explicit bypass confirmation and application-session scope", async () => {
    invokeMock.mockResolvedValue({
      effectivePermissionMode: "bypass",
      scope: "application-session",
      persisted: false,
      bypassActive: true
    });

    await permissionModeSet({
      mode: "bypass",
      bypassConfirmed: true,
      scope: "application-session"
    });

    expect(invokeMock).toHaveBeenCalledWith("permission_mode_set", {
      mode: "bypass",
      bypassConfirmed: true,
      bypass_confirmed: true,
      scope: "application-session"
    });
  });
});
