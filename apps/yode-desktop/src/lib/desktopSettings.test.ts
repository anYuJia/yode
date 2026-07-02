import { afterEach, describe, expect, it, vi } from "vitest";

import { loadConfigurationSettings, loadGeneralSettings, loadGeneralSettingsPayload, saveConfigurationSettings } from "./desktopSettings";

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

  it("loads configuration settings defaults from local storage", () => {
    stubLocalStorage(() => null);

    expect(loadConfigurationSettings()).toEqual({
      scope: "User config",
      approvalPolicy: "On request",
      sandboxSettings: "Read only",
      exposeDependencies: true
    });
  });

  it("loads configuration settings overrides", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-config-scope": "Project config",
        "yode-config-approval": "Never approve",
        "yode-config-sandbox": "Full write access",
        "yode-expose-deps": "false"
      };
      return values[key] ?? null;
    });

    expect(loadConfigurationSettings()).toEqual({
      scope: "Project config",
      approvalPolicy: "Never approve",
      sandboxSettings: "Full write access",
      exposeDependencies: false
    });
  });

  it("saves configuration settings through the shared helper", () => {
    const saved = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      setItem: (key: string, value: string) => saved.set(key, value)
    });

    saveConfigurationSettings({
      scope: "Project config",
      approvalPolicy: "Always auto-approve",
      sandboxSettings: "Restricted",
      exposeDependencies: false
    });

    expect(Object.fromEntries(saved)).toEqual({
      "yode-config-scope": "Project config",
      "yode-config-approval": "Always auto-approve",
      "yode-config-sandbox": "Restricted",
      "yode-expose-deps": "false"
    });
  });
});

function stubLocalStorage(getItem: (key: string) => string | null) {
  vi.stubGlobal("localStorage", {
    getItem
  });
}
