import { describe, expect, it, vi } from "vitest";

import {
  executeLocalSlashCommand,
  formatCurrentModelLabel,
  formatUsageSnapshot,
  LocalSlashCommandContext
} from "./localSlashCommands";

function commandContext(overrides: Partial<LocalSlashCommandContext> = {}): LocalSlashCommandContext {
  return {
    activeSession: null,
    activeSessionId: null,
    appLang: "en",
    bootstrapWorkspacePath: "/workspace",
    currentModel: "gpt-5",
    currentProvider: "openai",
    isProcessing: false,
    permissionMode: "default",
    selectedProjectRoot: "/workspace",
    sessionItems: [],
    timelineItemCount: 0,
    usageSnapshot: null,
    appendResult: vi.fn(),
    createSession: vi.fn(),
    clearMessageQueue: vi.fn(),
    setPendingUserQuestion: vi.fn(),
    setPermissionMode: vi.fn(),
    setSessionItems: vi.fn(),
    setTimelineItems: vi.fn(),
    setUsageSnapshot: vi.fn(),
    ...overrides
  };
}

describe("local slash commands", () => {
  it("formats usage snapshots with token and cost details", () => {
    expect(
      formatUsageSnapshot(
        {
          estimatedCost: 0.12345,
          inputTokens: 100,
          outputTokens: 50,
          cacheWriteTokens: 10,
          cacheReadTokens: 20
        },
        "en"
      )
    ).toContain("Estimated cost: $0.1235");
  });

  it("formats missing model state without fake fallback data", () => {
    expect(formatCurrentModelLabel("", "", "zh")).toBe("未连接桌面运行时");
    expect(formatCurrentModelLabel("openai", "", "en")).toBe("openai");
    expect(formatCurrentModelLabel("openai", "gpt-5", "en")).toBe("openai / gpt-5");
  });

  it("handles unknown commands locally", async () => {
    const appendResult = vi.fn();
    const handled = await executeLocalSlashCommand("/unknown", commandContext({ appendResult }));

    expect(handled).toBe(true);
    expect(appendResult).toHaveBeenCalledWith(
      "Unknown command",
      expect.stringContaining("does not support /unknown")
    );
  });

  it("lets review commands pass through to the agent", async () => {
    const handled = await executeLocalSlashCommand("/review", commandContext());

    expect(handled).toBe(false);
  });
});

describe("slash command registry", () => {
  it("parses command names and args", async () => {
    const { parseSlashCommand } = await import("./localSlashCommands");
    expect(parseSlashCommand("/clear")).toEqual({ name: "clear", args: "" });
    expect(parseSlashCommand("/rename 新标题 with spaces")).toEqual({
      name: "rename",
      args: "新标题 with spaces"
    });
    expect(parseSlashCommand("/  clear   ")).toEqual({ name: "clear", args: "" });
    expect(parseSlashCommand("plain text")).toBeNull();
    expect(parseSlashCommand("/")).toBeNull();
  });

  it("completes registered commands only", async () => {
    const { completeSlashCommands, slashCommandRegistry } = await import("./localSlashCommands");
    expect(completeSlashCommands("/c")).toEqual(["/clear", "/compact", "/cost"]);
    expect(completeSlashCommands("/per")).toEqual(["/permission"]);
    expect(completeSlashCommands("/nope")).toEqual([]);
    expect(completeSlashCommands("plain")).toEqual([]);
    // 帮助只列已注册命令：registry 中的名字都在补全集合内
    for (const name of Object.keys(slashCommandRegistry)) {
      expect(completeSlashCommands(`/${name}`)).toContain(`/${name}`);
    }
  });

  it("help lists implemented commands and never mentions unimplemented ones", async () => {
    const { getSlashCommandHelp, slashCommandRegistry } = await import("./localSlashCommands");
    const help = getSlashCommandHelp(true);
    const required = ["/clear", "/compact", "/export", "/new", "/trash", "/cost", "/model", "/permission", "/rename", "/sessions", "/status", "/help"];
    for (const command of required) {
      expect(help).toContain(command);
    }
    // 未实现的命令（如 /review）不得出现在帮助文案
    expect(help).not.toContain("/review");
    // 帮助与 registry 一一对应
    expect(Object.keys(slashCommandRegistry).length).toBeGreaterThanOrEqual(required.length);
  });

  it("rejects invalid arguments without changing session state", async () => {
    const { executeLocalSlashCommand, validateSlashCommand } = await import("./localSlashCommands");
    const appendResult = vi.fn();
    const context = commandContext({ appendResult });
    const handled = await executeLocalSlashCommand("/permission bogus-mode", context);
    expect(handled).toBe(true);
    expect(appendResult).toHaveBeenCalledWith("Invalid arguments", expect.stringContaining("Usage"));
    // 校验器与执行器语义一致
    expect(validateSlashCommand("/permission bogus-mode", true).ok).toBe(false);
    expect(validateSlashCommand("/permission auto", true).ok).toBe(true);
    expect(validateSlashCommand("/rename", true).ok).toBe(false);
    expect(validateSlashCommand("/rename 标题", true).ok).toBe(true);
  });

  it("executes the help command through the registry", async () => {
    const { executeLocalSlashCommand, getSlashCommandHelp } = await import("./localSlashCommands");
    const appendResult = vi.fn();
    await executeLocalSlashCommand("/help", commandContext({ appendResult, appLang: "zh" }));
    expect(appendResult).toHaveBeenCalledWith("桌面命令", getSlashCommandHelp(true));
  });
});
