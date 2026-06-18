import { afterEach, describe, expect, it, vi } from "vitest";

import {
  dedupeProjectRoots,
  loadStoredSelectedProjectRoot,
  SELECTED_PROJECT_ROOT_STORAGE_KEY,
  STANDALONE_PROJECT_SENTINEL,
  visibleSessions
} from "./projectStorage";
import { SessionSummary } from "./desktopTypes";

describe("project storage helpers", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("deduplicates and normalizes project roots", () => {
    expect(dedupeProjectRoots([" /repo ", "/repo", "", null, "/other"])).toEqual([
      "/repo",
      "/other"
    ]);
  });

  it("loads standalone selected project sentinel", () => {
    stubLocalStorage((key) =>
      key === SELECTED_PROJECT_ROOT_STORAGE_KEY ? STANDALONE_PROJECT_SENTINEL : null
    );

    expect(loadStoredSelectedProjectRoot()).toBeNull();
  });

  it("filters archived sessions from visible sessions", () => {
    stubLocalStorage((key) =>
      key === "yode-archived-session-ids" ? JSON.stringify(["hidden"]) : null
    );
    const sessions = [
      session("visible", "Visible"),
      session("hidden", "Hidden")
    ];

    expect(visibleSessions(sessions).map((item) => item.id)).toEqual(["visible"]);
  });
});

function session(id: string, title: string): SessionSummary {
  return {
    id,
    title,
    updatedAt: new Date(0).toISOString()
  };
}

function stubLocalStorage(getItem: (key: string) => string | null) {
  vi.stubGlobal("localStorage", {
    getItem
  });
}
