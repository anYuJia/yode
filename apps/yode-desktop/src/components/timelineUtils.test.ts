import { describe, expect, it } from "vitest";
import { TimelineItem } from "../lib/mock";
import { applyDesktopEventToTimelineItems, compileInlineItems } from "./timelineUtils";

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
});
