import { afterEach, describe, expect, it, vi } from "vitest";

import {
  computePaneDragSize,
  isPaneCollapsed,
  loadInitialPaneSize,
  SIDEBAR_WIDTH_STORAGE_KEY
} from "./paneLayout";

const drag = {
  startX: 100,
  startY: 100,
  startSidebarWidth: 240,
  startInspectorWidth: 280,
  startTerminalHeight: 300
};

describe("pane layout helpers", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("loads and clamps stored pane sizes", () => {
    stubLocalStorage((key) => (key === SIDEBAR_WIDTH_STORAGE_KEY ? "999" : null));

    expect(loadInitialPaneSize("sidebar", SIDEBAR_WIDTH_STORAGE_KEY)).toBe(420);
  });

  it("computes sidebar and inspector drag sizes from horizontal movement", () => {
    expect(computePaneDragSize("sidebar", drag, { clientX: 130, clientY: 100 }, 800)).toBe(270);
    expect(computePaneDragSize("inspector", drag, { clientX: 130, clientY: 100 }, 800)).toBe(250);
  });

  it("computes terminal drag size from vertical movement and viewport limit", () => {
    expect(computePaneDragSize("terminal", drag, { clientX: 100, clientY: 20 }, 400)).toBe(300);
    expect(computePaneDragSize("terminal", drag, { clientX: 100, clientY: 400 }, 800)).toBe(180);
  });

  it("detects collapsed panes at the minimum threshold", () => {
    expect(isPaneCollapsed("sidebar", 188)).toBe(true);
    expect(isPaneCollapsed("sidebar", 189)).toBe(false);
  });
});

function stubLocalStorage(getItem: (key: string) => string | null) {
  vi.stubGlobal("localStorage", {
    getItem
  });
}
