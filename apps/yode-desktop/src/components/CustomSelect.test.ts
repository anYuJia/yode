import { describe, expect, it } from "vitest";

import { customSelectKeyboardAction } from "./CustomSelect";

describe("CustomSelect keyboard navigation", () => {
  it("keeps Arrow navigation within the first and last option", () => {
    expect(customSelectKeyboardAction("ArrowUp", 0, 3)).toEqual({ type: "highlight", index: 0 });
    expect(customSelectKeyboardAction("ArrowDown", 2, 3)).toEqual({ type: "highlight", index: 2 });
    expect(customSelectKeyboardAction("Home", 2, 3)).toEqual({ type: "highlight", index: 0 });
    expect(customSelectKeyboardAction("End", 0, 3)).toEqual({ type: "highlight", index: 2 });
  });

  it("selects the highlighted option and closes with Escape", () => {
    expect(customSelectKeyboardAction("Enter", 1, 3)).toEqual({ type: "select", index: 1 });
    expect(customSelectKeyboardAction(" ", 1, 3)).toEqual({ type: "select", index: 1 });
    expect(customSelectKeyboardAction("Escape", 1, 3)).toEqual({ type: "close" });
  });
});
