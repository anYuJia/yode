import { describe, expect, it } from "vitest";
import { buildActivityGroupLabel } from "./ActivityGroupNode";
import { activityGroupPreview, getActivityDescriptor, summarizeActivityItems } from "./ToolUtils";

describe("ToolUtils activity descriptors", () => {
  it("prefers structured activity metadata", () => {
    const descriptor = getActivityDescriptor({
      tool: "exec_command",
      title: "运行命令",
      body: "{\"cmd\":\"git status --short\"}",
      metadata: {
        activity: {
          kind: "run",
          label: "Run git status --short",
          target: "git status --short",
          command: "git status --short"
        }
      }
    });

    expect(descriptor.kind).toBe("run");
    expect(descriptor.command).toBe("git status --short");
    expect(descriptor.target).toBe("git status --short");
  });

  it("falls back to existing file metadata", () => {
    const descriptor = getActivityDescriptor({
      tool: "read_file",
      title: "查看文件",
      body: "",
      metadata: {
        file_path: "/Users/pyu/code/yode/apps/yode-desktop/src/styles/app.css",
        start_line: 1,
        end_line: 20
      }
    });

    expect(descriptor.kind).toBe("read");
    expect(descriptor.filename).toBe("app.css");
    expect(descriptor.lineRange).toBe("#L1-20");
  });

  it("parses common command parameters from tool body", () => {
    const descriptor = getActivityDescriptor({
      tool: "exec_command",
      title: "运行命令",
      body: JSON.stringify({ cmd: "pnpm --dir apps/yode-desktop test -- --run" })
    });

    expect(descriptor.kind).toBe("run");
    expect(descriptor.command).toBe("pnpm --dir apps/yode-desktop test -- --run");
    expect(descriptor.target).toBe("pnpm --dir apps/yode-desktop test -- --run");
  });

  it("deduplicates repeated tool items by descriptor target", () => {
    const items = summarizeActivityItems([
      {
        kind: "tool",
        tool: "read_file",
        title: "查看文件",
        body: "",
        metadata: { activity: { kind: "read", target: "app.css", file_path: "app.css" } }
      },
      {
        kind: "tool",
        tool: "read_file",
        title: "查看文件",
        body: "",
        metadata: { activity: { kind: "read", target: "app.css", file_path: "app.css" } }
      }
    ]);

    expect(items).toHaveLength(1);
    expect(items[0].count).toBe(2);
  });

  it("builds an action-oriented preview from real tool targets", () => {
    const items = summarizeActivityItems([
      {
        kind: "tool",
        tool: "read_file",
        title: "查看文件",
        body: JSON.stringify({ file_path: "/Users/pyu/code/yode/Cargo.toml" }),
        status: "success"
      },
      {
        kind: "tool",
        tool: "grep",
        title: "内容搜索",
        body: JSON.stringify({ pattern: "activityGroupPreview", path: "apps/yode-desktop/src" }),
        status: "success"
      },
      {
        kind: "tool",
        tool: "exec_command",
        title: "运行命令",
        body: JSON.stringify({ cmd: "git status --short" }),
        status: "success"
      }
    ]);

    expect(activityGroupPreview(items, "zh")).toBe(
      "查看 Cargo.toml，搜索 activityGroupPreview，运行 git status --short"
    );
  });

  it("separates completed group label parts in Chinese", () => {
    const items = summarizeActivityItems([
      {
        kind: "tool",
        tool: "read_file",
        title: "查看文件",
        body: JSON.stringify({ file_path: "/Users/pyu/code/yode/Cargo.toml" }),
        status: "success"
      },
      {
        kind: "tool",
        tool: "exec_command",
        title: "运行命令",
        body: JSON.stringify({ cmd: "git status --short" }),
        status: "success"
      }
    ]);

    expect(buildActivityGroupLabel(items, "zh", false)).toBe("已查看 1 个文件，已运行 1 条命令");
  });
});
