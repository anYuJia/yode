import { afterEach, describe, expect, it, vi } from "vitest";

import { loadGeneralSettings, loadGeneralSettingsPayload } from "./desktopSettings";

describe("desktop settings helpers", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("loads general settings defaults from local storage", () => {
    stubLocalStorage(() => null);

    expect(loadGeneralSettings()).toEqual({
      bottomPanel: true,
      suggestedPrompts: true,
      contextUsage: false,
      requireOptEnter: false,
      followUpBehavior: "queue",
      codeReviewPolicy: "inline",
      completionNotification: "Only when unfocused",
      permissionNotification: true,
      questionNotification: true
    });
  });

  it("loads general settings payload overrides", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-work-mode": "planning",
        "yode-def-perm": "false",
        "yode-auto-review": "false",
        "yode-full-access": "false",
        "yode-open-dest": "Cursor",
        "yode-show-menu-bar": "false",
        "yode-bottom-panel": "false",
        "yode-term-loc": "right",
        "yode-prevent-sleep": "true",
        "yode-code-review-policy": "summary",
        "yode-suggested-prompts": "false",
        "yode-context-usage": "true",
        "yode-follow-up-behavior": "interrupt",
        "yode-require-opt-enter": "true",
        "yode-completion-notif": "Never",
        "yode-perm-notif": "false",
        "yode-question-notif": "false"
      };
      return values[key] ?? null;
    });

    expect(loadGeneralSettingsPayload()).toEqual({
      workMode: "planning",
      defaultFilePermission: false,
      autoReview: false,
      fullAccess: false,
      openDestination: "Cursor",
      showInMenuBar: false,
      bottomPanel: false,
      terminalLocation: "right",
      preventSleep: true,
      codeReviewPolicy: "summary",
      suggestedPrompts: false,
      contextUsage: true,
      followUpBehavior: "interrupt",
      requireOptEnter: true,
      completionNotification: "Never",
      permissionNotification: false,
      questionNotification: false
    });
  });
});

function stubLocalStorage(getItem: (key: string) => string | null) {
  vi.stubGlobal("localStorage", {
    getItem
  });
}
