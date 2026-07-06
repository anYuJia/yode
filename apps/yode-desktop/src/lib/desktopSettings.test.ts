import { afterEach, describe, expect, it, vi } from "vitest";

import {
  DEFAULT_BROWSER_SETTINGS,
  DEFAULT_COMPUTER_USE_SETTINGS,
  DEFAULT_GIT_SETTINGS,
  DEFAULT_HOOKS,
  DEFAULT_HOOKS_SETTINGS,
  DEFAULT_MCP_SERVERS,
  DEFAULT_PERSONALIZATION_SETTINGS,
  loadBrowserSettings,
  loadComputerUseSettings,
  loadGitSettings,
  loadHooksSettings,
  loadMcpServers,
  loadConfigurationSettings,
  loadGeneralSettings,
  loadGeneralSettingsPayload,
  loadPersonalizationSettings,
  loadWorktreesSettings,
  saveBrowserSettings,
  saveComputerUseSettings,
  saveConfigurationSettings,
  saveGitSettings,
  saveHooksSettings,
  saveMcpServers,
  savePersonalizationSetting,
  savePersonalizationSettings,
  saveWorktreesSetting
} from "./desktopSettings";

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

  it("loads worktrees settings defaults from local storage", () => {
    stubLocalStorage(() => null);

    expect(loadWorktreesSettings()).toEqual({
      baseDir: "~/.yode/worktrees",
      autoDeleteOnSessionEnd: true,
      preserveUncommitted: true,
      cleanUnusedCache: false
    });
  });

  it("loads worktrees settings overrides", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-worktrees-base-dir": "/tmp/yode-worktrees",
        "yode-worktrees-auto-delete-session-end": "false",
        "yode-worktrees-preserve-uncommitted": "false",
        "yode-worktrees-clean-unused-cache": "true"
      };
      return values[key] ?? null;
    });

    expect(loadWorktreesSettings()).toEqual({
      baseDir: "/tmp/yode-worktrees",
      autoDeleteOnSessionEnd: false,
      preserveUncommitted: false,
      cleanUnusedCache: true
    });
  });

  it("saves worktrees settings through mapped keys", async () => {
    const saved = new Map<string, string>();
    vi.stubGlobal("window", {});
    vi.stubGlobal("localStorage", {
      setItem: (key: string, value: string) => saved.set(key, value)
    });

    await saveWorktreesSetting("baseDir", "/tmp/yode-worktrees");
    await saveWorktreesSetting("autoDeleteOnSessionEnd", false);
    await saveWorktreesSetting("preserveUncommitted", false);
    await saveWorktreesSetting("cleanUnusedCache", true);

    expect(Object.fromEntries(saved)).toEqual({
      "yode-worktrees-base-dir": "/tmp/yode-worktrees",
      "yode-worktrees-auto-delete-session-end": "false",
      "yode-worktrees-preserve-uncommitted": "false",
      "yode-worktrees-clean-unused-cache": "true"
    });
  });

  it("loads git settings defaults from local storage", () => {
    stubLocalStorage(() => null);

    expect(loadGitSettings()).toEqual(DEFAULT_GIT_SETTINGS);
  });

  it("loads git settings overrides", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-git-branch-prefix": "codex/",
        "yode-git-merge-method": "squash",
        "yode-git-show-pr-icons": "false",
        "yode-git-always-force-push": "true",
        "yode-git-create-draft-prs": "false",
        "yode-git-auto-delete-worktrees": "false",
        "yode-git-auto-delete-limit": "7",
        "yode-git-commit-instructions": "Use conventional commits",
        "yode-git-pr-instructions": "Include screenshots"
      };
      return values[key] ?? null;
    });

    expect(loadGitSettings()).toEqual({
      branchPrefix: "codex/",
      mergeMethod: "squash",
      showPrIcons: false,
      alwaysForcePush: true,
      createDraftPrs: false,
      autoDeleteWorktrees: false,
      autoDeleteLimit: 7,
      commitInstructions: "Use conventional commits",
      prInstructions: "Include screenshots"
    });
  });

  it("saves git settings through the shared helper", () => {
    const saved = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      setItem: (key: string, value: string) => saved.set(key, value)
    });

    saveGitSettings({
      branchPrefix: "codex/",
      mergeMethod: "squash",
      showPrIcons: false,
      alwaysForcePush: true,
      createDraftPrs: false,
      autoDeleteWorktrees: false,
      autoDeleteLimit: 7,
      commitInstructions: "Use conventional commits",
      prInstructions: "Include screenshots"
    });

    expect(Object.fromEntries(saved)).toEqual({
      "yode-git-branch-prefix": "codex/",
      "yode-git-merge-method": "squash",
      "yode-git-show-pr-icons": "false",
      "yode-git-always-force-push": "true",
      "yode-git-create-draft-prs": "false",
      "yode-git-auto-delete-worktrees": "false",
      "yode-git-auto-delete-limit": "7",
      "yode-git-commit-instructions": "Use conventional commits",
      "yode-git-pr-instructions": "Include screenshots"
    });
  });

  it("loads browser settings defaults from local storage", () => {
    stubLocalStorage(() => null);

    expect(loadBrowserSettings()).toEqual(DEFAULT_BROWSER_SETTINGS);
  });

  it("loads browser settings overrides and filters invalid domain entries", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-browser-enabled": "false",
        "yode-browser-annotation-screenshots": "Ask each time",
        "yode-browser-approval": "Always allow",
        "yode-browser-blocked-domains": JSON.stringify(["blocked.example", 123, null]),
        "yode-browser-allowed-domains": JSON.stringify(["allowed.example"])
      };
      return values[key] ?? null;
    });

    expect(loadBrowserSettings()).toEqual({
      enabled: false,
      annotationScreenshots: "Ask each time",
      approvalPolicy: "Always allow",
      blockedDomains: ["blocked.example"],
      allowedDomains: ["allowed.example"]
    });
  });

  it("falls back to empty browser domain lists on malformed storage", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-browser-blocked-domains": "{broken",
        "yode-browser-allowed-domains": JSON.stringify({ domain: "example.com" })
      };
      return values[key] ?? null;
    });

    expect(loadBrowserSettings()).toEqual(DEFAULT_BROWSER_SETTINGS);
  });

  it("saves browser settings through the shared helper", () => {
    const saved = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      setItem: (key: string, value: string) => saved.set(key, value)
    });

    saveBrowserSettings({
      enabled: false,
      annotationScreenshots: "Never include",
      approvalPolicy: "Never allow",
      blockedDomains: ["blocked.example"],
      allowedDomains: ["allowed.example"]
    });

    expect(Object.fromEntries(saved)).toEqual({
      "yode-browser-enabled": "false",
      "yode-browser-annotation-screenshots": "Never include",
      "yode-browser-approval": "Never allow",
      "yode-browser-blocked-domains": JSON.stringify(["blocked.example"]),
      "yode-browser-allowed-domains": JSON.stringify(["allowed.example"])
    });
  });

  it("loads personalization settings defaults from local storage", () => {
    stubLocalStorage(() => null);

    expect(loadPersonalizationSettings()).toEqual(DEFAULT_PERSONALIZATION_SETTINGS);
  });

  it("loads personalization settings overrides", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-personality": "Concise",
        "yode-custom-instructions": "Prefer direct answers",
        "yode-enable-memories": "true",
        "yode-skip-tool-chats": "true"
      };
      return values[key] ?? null;
    });

    expect(loadPersonalizationSettings()).toEqual({
      personality: "Concise",
      customInstructions: "Prefer direct answers",
      enableMemories: true,
      skipToolChats: true
    });
  });

  it("saves personalization settings through the shared helper", () => {
    const saved = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      setItem: (key: string, value: string) => saved.set(key, value)
    });

    savePersonalizationSettings({
      personality: "Professional",
      customInstructions: "Use Simplified Chinese",
      enableMemories: true,
      skipToolChats: false
    });

    expect(Object.fromEntries(saved)).toEqual({
      "yode-personality": "Professional",
      "yode-custom-instructions": "Use Simplified Chinese",
      "yode-enable-memories": "true",
      "yode-skip-tool-chats": "false"
    });
  });

  it("saves single personalization settings through mapped keys", async () => {
    const saved = new Map<string, string>();
    vi.stubGlobal("window", {});
    vi.stubGlobal("localStorage", {
      setItem: (key: string, value: string) => saved.set(key, value)
    });

    await savePersonalizationSetting("personality", "Friendly");
    await savePersonalizationSetting("customInstructions", "Keep it short");
    await savePersonalizationSetting("enableMemories", true);
    await savePersonalizationSetting("skipToolChats", true);

    expect(Object.fromEntries(saved)).toEqual({
      "yode-personality": "Friendly",
      "yode-custom-instructions": "Keep it short",
      "yode-enable-memories": "true",
      "yode-skip-tool-chats": "true"
    });
  });

  it("loads computer use settings defaults from local storage", () => {
    stubLocalStorage(() => null);

    expect(loadComputerUseSettings()).toEqual(DEFAULT_COMPUTER_USE_SETTINGS);
  });

  it("loads computer use settings overrides and filters invalid apps", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-computer-use-anyapp": "installed",
        "yode-computer-use-chrome": "installing",
        "yode-computer-use-allowed-apps": JSON.stringify(["Slack", 42, null, "Finder"])
      };
      return values[key] ?? null;
    });

    expect(loadComputerUseSettings()).toEqual({
      anyAppStatus: "installed",
      chromeStatus: "installing",
      allowedApps: ["Slack", "Finder"]
    });
  });

  it("falls back to uninstalled status and empty apps on malformed computer use storage", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-computer-use-anyapp": "bad-status",
        "yode-computer-use-chrome": "connected",
        "yode-computer-use-allowed-apps": "{broken"
      };
      return values[key] ?? null;
    });

    expect(loadComputerUseSettings()).toEqual(DEFAULT_COMPUTER_USE_SETTINGS);
  });

  it("saves computer use settings through the shared helper", () => {
    const saved = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      setItem: (key: string, value: string) => saved.set(key, value)
    });

    saveComputerUseSettings({
      anyAppStatus: "installed",
      chromeStatus: "uninstalled",
      allowedApps: ["Slack", "Finder"]
    });

    expect(Object.fromEntries(saved)).toEqual({
      "yode-computer-use-anyapp": "installed",
      "yode-computer-use-chrome": "uninstalled",
      "yode-computer-use-allowed-apps": JSON.stringify(["Slack", "Finder"])
    });
  });

  it("loads hooks settings defaults from local storage", () => {
    stubLocalStorage(() => null);

    expect(loadHooksSettings()).toEqual(DEFAULT_HOOKS_SETTINGS);
  });

  it("loads hooks settings overrides and normalizes snake case fields", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-hooks-enabled": "false",
        "yode-hooks-list": JSON.stringify([
          {
            name: "Run tests",
            events: ["pre_turn"],
            command: "pnpm test",
            timeout_secs: 30,
            can_block: true,
            disabled: true,
            tool_filter: ["bash", ""]
          },
          { name: "", events: [], command: "" }
        ])
      };
      return values[key] ?? null;
    });

    expect(loadHooksSettings()).toEqual({
      enabled: false,
      hooks: [
        {
          name: "Run tests",
          events: ["pre_turn"],
          command: "pnpm test",
          timeoutSecs: 30,
          canBlock: true,
          disabled: true,
          toolFilter: ["bash"]
        }
      ]
    });
  });

  it("falls back to default hooks on malformed hook storage", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-hooks-list": "{broken"
      };
      return values[key] ?? null;
    });

    expect(loadHooksSettings()).toEqual(DEFAULT_HOOKS_SETTINGS);
  });

  it("saves hooks settings through the shared helper", () => {
    const saved = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      setItem: (key: string, value: string) => saved.set(key, value)
    });

    saveHooksSettings({
      enabled: false,
      hooks: [
        {
          name: "Format",
          events: ["task_completed"],
          command: "cargo fmt",
          timeoutSecs: 10,
          canBlock: false,
          disabled: false
        }
      ]
    });

    expect(saved.get("yode-hooks-enabled")).toBe("false");
    expect(JSON.parse(saved.get("yode-hooks-list") || "[]")).toEqual([
      {
        name: "Format",
        events: ["task_completed"],
        command: "cargo fmt",
        timeoutSecs: 10,
        canBlock: false,
        disabled: false
      }
    ]);
  });

  it("uses default hooks when stored hook list normalizes empty", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-hooks-list": JSON.stringify([{ name: "", events: [], command: "" }, null, ["bad"]])
      };
      return values[key] ?? null;
    });

    expect(loadHooksSettings()).toEqual({
      enabled: true,
      hooks: DEFAULT_HOOKS
    });
  });

  it("loads mcp server defaults from local storage", () => {
    stubLocalStorage(() => null);

    expect(loadMcpServers()).toEqual(DEFAULT_MCP_SERVERS);
  });

  it("loads mcp servers and filters invalid entries", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-mcp-servers": JSON.stringify([
          {
            name: "node_repl",
            transport: "stdio",
            command: "node",
            args: ["--eval", 123],
            env: { NODE_ENV: "test", PORT: 3000 },
            disabled: false
          },
          {
            name: "docs",
            transport: "http",
            url: "https://example.com/mcp",
            disabled: true
          },
          { name: "", transport: "stdio", command: "node" },
          { name: "bad", transport: "smtp", url: "x" },
          { name: "missing-command", transport: "stdio" },
          null,
          ["not-a-server"]
        ])
      };
      return values[key] ?? null;
    });

    expect(loadMcpServers()).toEqual([
      {
        name: "node_repl",
        transport: "stdio",
        command: "node",
        args: ["--eval", "123"],
        env: { NODE_ENV: "test", PORT: "3000" },
        disabled: false
      },
      {
        name: "docs",
        transport: "http",
        url: "https://example.com/mcp",
        disabled: true
      }
    ]);
  });

  it("falls back to default mcp servers on malformed storage", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        "yode-mcp-servers": "{broken"
      };
      return values[key] ?? null;
    });

    expect(loadMcpServers()).toEqual(DEFAULT_MCP_SERVERS);
  });

  it("saves normalized mcp servers through the shared helper", () => {
    const saved = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      setItem: (key: string, value: string) => saved.set(key, value)
    });

    saveMcpServers([
      {
        name: "node_repl",
        transport: "stdio",
        command: "node",
        args: ["--eval"],
        env: { NODE_ENV: "test" },
        disabled: false
      },
      { name: "invalid", transport: "stdio", disabled: false }
    ]);

    expect(JSON.parse(saved.get("yode-mcp-servers") || "[]")).toEqual([
      {
        name: "node_repl",
        transport: "stdio",
        command: "node",
        args: ["--eval"],
        env: { NODE_ENV: "test" },
        disabled: false
      }
    ]);
  });
});

function stubLocalStorage(getItem: (key: string) => string | null) {
  vi.stubGlobal("localStorage", {
    getItem
  });
}
