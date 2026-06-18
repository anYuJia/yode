import { describe, expect, it, vi } from "vitest";

import {
  executeLocalSlashCommand,
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
