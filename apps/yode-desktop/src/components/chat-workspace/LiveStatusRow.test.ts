import { describe, expect, it } from "vitest";
import { TimelineItem } from "../../lib/desktopTypes";
import { liveStatusTextForItems } from "./LiveStatusRow";

describe("LiveStatusRow status text", () => {
  it("shows thinking for running reasoning instead of rendering it in the timeline", () => {
    const items: TimelineItem[] = [
      {
        id: "reasoning-1",
        kind: "reasoning",
        title: "思考中...",
        body: "",
        meta: "running"
      }
    ];

    expect(liveStatusTextForItems(items, "zh")).toBe("思考中...");
  });

  it("does not reuse completed action narrative body as the live status", () => {
    const items: TimelineItem[] = [
      {
        id: "action-narrative-turn-1-1",
        kind: "process_note",
        body: "现在我对现有项目有了清晰的了解。",
        status: "success"
      }
    ];

    expect(liveStatusTextForItems(items, "zh")).toBe("");
  });
});
