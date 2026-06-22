import { describe, expect, it } from "vitest";
import { TimelineItem } from "../lib/desktopTypes";
import { activityGroupPreview, summarizeActivityItems } from "./activity/ToolUtils";
import { applyDesktopEventToTimelineItems, compileInlineItems, messagesToTimelineItems, splitTurnVisibleItems } from "./timelineUtils";

function expectActivityGroup(item: TimelineItem): Extract<TimelineItem, { kind: "activity_group" }> {
  expect(item.kind).toBe("activity_group");
  return item as Extract<TimelineItem, { kind: "activity_group" }>;
}

const tool = (
  id: string,
  activity: Record<string, unknown>,
  status: "running" | "success" | "blocked" = "success"
): Extract<TimelineItem, { kind: "tool" }> => ({
  id,
  kind: "tool",
  title: String(activity.label || "工具"),
  body: "",
  tool: String(activity.tool || "tool"),
  status,
  metadata: { activity }
});

describe("timeline activity grouping", () => {
  it("groups mixed read and run activity into one Codex-style activity group", () => {
    const grouped = compileInlineItems([
      tool("read-1", { kind: "read", target: "app.css", file_path: "app.css", tool: "read_file" }),
      tool("read-2", { kind: "read", target: "Sidebar.tsx", file_path: "Sidebar.tsx", tool: "read_file" }),
      tool("run-1", { kind: "run", target: "git status --short", command: "git status --short", tool: "exec_command" })
    ]);

    expect(grouped).toHaveLength(1);
    expect(grouped[0].kind).toBe("activity_group");
    expect(grouped[0]).toMatchObject({
      type: "mixed",
      status: "success",
      label: "已探索 2 个文件已运行 1 条命令"
    });
  });

  it("does not render completed reasoning as separate process rows", () => {
    const grouped = compileInlineItems([
      {
        id: "reasoning-1",
        kind: "reasoning",
        title: "已思考 1 秒",
        body: "private reasoning",
        meta: "complete"
      },
      tool("read-1", { kind: "read", target: "Cargo.toml", file_path: "Cargo.toml", tool: "read_file" }),
      {
        id: "reasoning-2",
        kind: "reasoning",
        title: "已思考 1 秒",
        body: "private reasoning",
        meta: "complete"
      },
      tool("read-2", { kind: "read", target: "tauri.conf.json", file_path: "tauri.conf.json", tool: "read_file" })
    ]);

    expect(grouped).toHaveLength(1);
    expect(grouped[0]).toMatchObject({
      kind: "activity_group",
      type: "explore",
      label: "已探索 2 个文件"
    });
  });

  it("merges tool_result into the matching tool_started item", () => {
    const started = applyDesktopEventToTimelineItems([], {
      kind: "tool_started",
      turnId: "turn-1",
      payload: {
        id: "call-1",
        title: "运行命令",
        tool: "exec_command",
        body: "{\"cmd\":\"git status --short\"}",
        status: "running"
      }
    });

    const completed = applyDesktopEventToTimelineItems(started, {
      kind: "tool_result",
      turnId: "turn-1",
      payload: {
        id: "call-1",
        title: "工具结果",
        tool: "exec_command",
        body: " M app.css",
        status: "success",
        metadata: {
          activity: { kind: "run", target: "git status --short", command: "git status --short" }
        }
      }
    });

    expect(completed).toHaveLength(1);
    expect(completed[0]).toMatchObject({
      kind: "tool",
      status: "success",
      result: " M app.css",
      metadata: {
        activity: { kind: "run", target: "git status --short", command: "git status --short" }
      }
    });
  });

  it("keeps run errors as error nodes instead of assistant answers", () => {
    const items = applyDesktopEventToTimelineItems([
      {
        id: "assistant-turn-1-0",
        kind: "assistant",
        title: "Yode",
        body: "正在准备请求",
        meta: "streaming"
      }
    ], {
      kind: "error",
      turnId: "turn-1",
      payload: {
        title: "错误",
        body: "Request failed after 1 attempt: OpenAI API error (400 Bad Request): Param Incorrect (code: 400)"
      }
    });

    expect(items.some((item) => item.kind === "error")).toBe(true);
    expect(items.filter((item) => item.kind === "assistant")).toHaveLength(1);
    expect(items.find((item) => item.kind === "assistant")).toMatchObject({
      meta: "stream complete"
    });
  });

  it("keeps error nodes visible outside collapsed process items", () => {
    const { processItems, answerItems } = splitTurnVisibleItems([
      {
        id: "reasoning-turn-1",
        kind: "reasoning",
        title: "已思考",
        body: "",
        meta: "complete"
      },
      {
        id: "error-turn-1",
        kind: "error",
        title: "错误",
        body: "Request failed after 1 attempt"
      }
    ]);

    expect(processItems).toHaveLength(1);
    expect(answerItems).toHaveLength(1);
    expect(answerItems[0]).toMatchObject({ kind: "error" });
  });

  it("renders batch invocations instead of exposing the batch wrapper", () => {
    const batchItem: Extract<TimelineItem, { kind: "tool" }> = {
      id: "batch-1",
      kind: "tool",
      title: "Batch",
      body: JSON.stringify({
        invocations: [
          { tool_name: "read_file", params: { file_path: "Cargo.toml" } },
          { tool_name: "grep", params: { pattern: "ActionNarrative", path: "crates" } }
        ]
      }),
      tool: "batch",
      status: "success"
    };
    const grouped = compileInlineItems([
      batchItem
    ]);
    const group = expectActivityGroup(grouped[0]);
    const visibleItems = summarizeActivityItems(group.items);
    const preview = activityGroupPreview(visibleItems, "zh");
    const visibleTools = visibleItems.filter((item): item is Extract<TimelineItem, { kind: "tool" }> => item.kind === "tool");

    expect(visibleTools.map((item) => item.tool)).toEqual(["read_file", "grep"]);
    expect(preview).toContain("Cargo.toml");
    expect(preview).not.toContain("batch");
    expect(grouped).toHaveLength(1);
    expect(grouped[0]).toMatchObject({
      kind: "activity_group",
      type: "other"
    });
  });

  it("keeps natural short action narratives visible", () => {
    const items = compileInlineItems([
      {
        id: "action-narrative-turn-1",
        kind: "process_note",
        body: "先看事件链路。",
        status: "success"
      }
    ]);

    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({
      kind: "process_note",
      body: "先看事件链路。"
    });
  });

  it("keeps all action narratives reviewable after completion", () => {
    const notes = Array.from({ length: 8 }, (_, index) => ({
      id: `action-narrative-turn-1-${index}`,
      kind: "process_note" as const,
      body: `第 ${index + 1} 步：检查相关上下文。`,
      status: "success" as const
    }));
    const grouped = compileInlineItems([
      ...notes,
      {
        id: "assistant-final",
        kind: "assistant",
        title: "Yode",
        body: "完成。",
        meta: "stream complete"
      }
    ], false, "zh");

    expect(grouped.filter((item) => item.kind === "process_note")).toHaveLength(8);
  });

  it("does not expose ask_user as a tool activity row", () => {
    const grouped = compileInlineItems([
      {
        id: "tool-ask",
        kind: "tool",
        title: "调用工具: ask_user",
        body: JSON.stringify({ questions: [{ header: "扫描目标", question: "选什么？" }] }),
        tool: "ask_user",
        status: "running"
      }
    ]);

    expect(grouped).toHaveLength(0);
  });

  it("keeps concise model action narratives even after a public preamble", () => {
    const items = applyDesktopEventToTimelineItems([
      {
        id: "assistant-turn-1-0",
        kind: "assistant",
        title: "Yode",
        body: "我先看一下项目结构。",
        meta: "streaming"
      }
    ], {
      kind: "action_narrative",
      turnId: "turn-1",
      payload: {
        id: "narrative-1",
        body: "检查项目结构"
      }
    });

    expect(items.some((item) => item.kind === "process_note" && item.body === "检查项目结构")).toBe(true);
  });

  it("merges write_file results into the original edit item with diff stats", () => {
    const grouped = compileInlineItems([
      {
        id: "tool-start-1",
        kind: "tool",
        title: "调用工具: write_file",
        body: JSON.stringify({ path: "1.md", content: "a\nb\nc" }),
        tool: "write_file",
        callId: "call-1",
        status: "running"
      },
      {
        id: "tool-result-1",
        kind: "tool",
        title: "Tool Result",
        body: "Successfully wrote 5 bytes",
        tool: "write_file",
        callId: "call-1",
        status: "success",
        metadata: {
          file_path: "1.md",
          diff_preview: {
            removed: [],
            added: ["a", "b", "c"],
            more_removed: 0,
            more_added: 0
          }
        }
      }
    ]);

    expect(grouped).toHaveLength(1);
    expect(grouped[0]).toMatchObject({
      kind: "edit_summary",
      items: [
        {
          filename: "1.md",
          diff: "+3 -0",
          result: "Successfully wrote 5 bytes"
        }
      ]
    });
  });

  it("keeps blocked write results from being counted as successful edits", () => {
    const grouped = compileInlineItems([
      {
        id: "tool-start-2",
        kind: "tool",
        title: "调用工具: write_file",
        body: JSON.stringify({ path: "ones.txt", content: "hello" }),
        tool: "write_file",
        callId: "call-2",
        status: "running"
      },
      {
        id: "tool-result-2",
        kind: "tool",
        title: "Tool Result",
        body: "You must read the file '/Users/pyu/code/transapi/ones.txt' with read_file before editing or overwriting it.",
        tool: "write_file",
        callId: "call-2",
        status: "blocked",
        metadata: {
          file_path: "ones.txt"
        }
      }
    ]);

    expect(grouped).toHaveLength(1);
    expect(grouped[0]).toMatchObject({
      kind: "edit_summary",
      status: "blocked",
      items: [
        {
          filename: "ones.txt",
          status: "blocked",
          result: "You must read the file '/Users/pyu/code/transapi/ones.txt' with read_file before editing or overwriting it."
        }
      ]
    });
  });

  it("does not turn aggregate budget notices into blocked edit summaries", () => {
    const grouped = compileInlineItems([
      {
        id: "tool-start-budget",
        kind: "tool",
        title: "调用工具: edit_file",
        body: JSON.stringify({ file_path: "registry.go", old_string: "old", new_string: "new" }),
        tool: "edit_file",
        callId: "call-budget",
        status: "running"
      },
      {
        id: "tool-result-budget",
        kind: "tool",
        title: "Tool Result",
        body: "[AGGREGATE BUDGET EXCEEDED: Full result (118 bytes) omitted to prevent context overflow. Summarize your current findings instead.]",
        tool: "edit_file",
        callId: "call-budget",
        status: "blocked"
      }
    ]);

    expect(grouped).toEqual([]);
  });

  it("restores structured tool result metadata from history", () => {
    const history = messagesToTimelineItems([
      {
        id: 1,
        role: "assistant",
        content: "",
        toolCallsJson: JSON.stringify([
          {
            id: "call-1",
            name: "exec_command",
            arguments: JSON.stringify({ cmd: "git status --short" })
          }
        ]),
        createdAt: "2026-06-13T10:00:00Z"
      },
      {
        id: 2,
        role: "tool",
        content: " M app.css",
        toolCallId: "call-1",
        metadata: {
          activity: {
            kind: "run",
            target: "git status --short",
            command: "git status --short"
          }
        },
        createdAt: "2026-06-13T10:00:01Z"
      }
    ]);

    const grouped = compileInlineItems(history, false, "zh");
    expect(grouped).toHaveLength(1);
    expect(grouped[0]).toMatchObject({
      kind: "activity_group",
      type: "run",
      label: "已运行 1 条命令"
    });
    expect(expectActivityGroup(grouped[0]).items[0]).toMatchObject({
      result: "M app.css",
      metadata: {
        activity: {
          command: "git status --short"
        }
      }
    });
  });
});
