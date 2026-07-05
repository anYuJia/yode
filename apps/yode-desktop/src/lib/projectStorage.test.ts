import { afterEach, describe, expect, it, vi } from "vitest";

import {
  ENVIRONMENT_PROJECTS_STORAGE_KEY,
  PROJECT_ORDER_STORAGE_KEY,
  PROJECT_ROOTS_CHANGED_EVENT,
  PROJECT_ROOTS_STORAGE_KEY,
  SESSION_DELETED_PERMANENTLY_EVENT,
  SESSION_UNARCHIVED_EVENT,
  SESSIONS_IMPORTED_EVENT,
  dedupeProjectRoots,
  detailFromSessionIdEvent,
  dispatchSessionDeletedPermanently,
  dispatchSessionUnarchived,
  dispatchSessionsImported,
  loadRealProjectRoots,
  loadStoredSelectedProjectRoot,
  loadStoredProjectEnvironments,
  normalizeProjectEnvironment,
  projectLabelFromPath,
  saveRealProjectRoots,
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

  it("derives a project label from posix and windows paths", () => {
    expect(projectLabelFromPath("/Users/pyu/code/yode")).toBe("yode");
    expect(projectLabelFromPath("C:\\Users\\pyu\\repo")).toBe("repo");
  });

  it("loads real project roots in saved order with dedupe", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        [PROJECT_ROOTS_STORAGE_KEY]: JSON.stringify(["/repo-b", "/repo-a", "/repo-b"]),
        [PROJECT_ORDER_STORAGE_KEY]: JSON.stringify(["/repo-a", "/repo-c"])
      };
      return values[key] ?? null;
    });

    expect(loadRealProjectRoots()).toEqual(["/repo-a", "/repo-b"]);
  });

  it("saves real project roots and dispatches a project roots event", () => {
    const saved = new Map<string, string>();
    const dispatched: string[] = [];
    vi.stubGlobal("localStorage", {
      setItem: (key: string, value: string) => saved.set(key, value)
    });
    vi.stubGlobal("window", {
      dispatchEvent: (event: Event) => {
        dispatched.push(event.type);
        return true;
      }
    });

    saveRealProjectRoots(["/repo", "/repo", " /other "]);

    expect(saved.get(PROJECT_ROOTS_STORAGE_KEY)).toBe(JSON.stringify(["/repo", "/other"]));
    expect(saved.get(PROJECT_ORDER_STORAGE_KEY)).toBe(JSON.stringify(["/repo", "/other"]));
    expect(dispatched).toEqual([PROJECT_ROOTS_CHANGED_EVENT]);
  });

  it("dispatches session lifecycle events through shared helpers", () => {
    const dispatched: Array<{ type: string; detail: unknown }> = [];
    vi.stubGlobal("window", {
      dispatchEvent: (event: CustomEvent) => {
        dispatched.push({ type: event.type, detail: event.detail });
        return true;
      }
    });

    dispatchSessionUnarchived("s-1");
    dispatchSessionDeletedPermanently("s-2");
    dispatchSessionsImported([{ id: "s-3", title: "Imported" } as never]);

    expect(dispatched).toEqual([
      { type: SESSION_UNARCHIVED_EVENT, detail: { sessionId: "s-1" } },
      { type: SESSION_DELETED_PERMANENTLY_EVENT, detail: { sessionId: "s-2" } },
      { type: SESSIONS_IMPORTED_EVENT, detail: [{ id: "s-3", title: "Imported" }] }
    ]);
  });

  it("guards session id event payloads", () => {
    expect(detailFromSessionIdEvent(new CustomEvent(SESSION_DELETED_PERMANENTLY_EVENT, {
      detail: { sessionId: "s-1" }
    }))).toEqual({ sessionId: "s-1" });
    expect(detailFromSessionIdEvent(new CustomEvent(SESSION_DELETED_PERMANENTLY_EVENT, {
      detail: { sessionId: "" }
    }))).toBeNull();
    expect(detailFromSessionIdEvent(new Event(SESSION_DELETED_PERMANENTLY_EVENT))).toBeNull();
  });

  it("normalizes project environment records", () => {
    expect(
      normalizeProjectEnvironment({
        name: "",
        path: " /Users/pyu/code/yode ",
        envVars: undefined
      })
    ).toEqual({
      name: "yode",
      subtext: "code",
      path: "/Users/pyu/code/yode",
      execMode: "host",
      setupCommand: "",
      envVars: []
    });
  });

  it("loads stored project environments merged with roots", () => {
    stubLocalStorage((key) => {
      const values: Record<string, string> = {
        [PROJECT_ROOTS_STORAGE_KEY]: JSON.stringify(["/repo-a", "/repo-b"]),
        [PROJECT_ORDER_STORAGE_KEY]: JSON.stringify(["/repo-b", "/repo-a"]),
        [ENVIRONMENT_PROJECTS_STORAGE_KEY]: JSON.stringify([
          {
            name: "Repo B",
            path: "/repo-b",
            setupCommand: "pnpm install",
            execMode: "docker"
          },
          {
            name: "Missing path"
          }
        ])
      };
      return values[key] ?? null;
    });

    expect(loadStoredProjectEnvironments()).toEqual([
      {
        name: "Repo B",
        subtext: undefined,
        path: "/repo-b",
        setupCommand: "pnpm install",
        execMode: "docker",
        envVars: []
      },
      {
        name: "repo-a",
        subtext: undefined,
        path: "/repo-a",
        setupCommand: "",
        execMode: "host",
        envVars: []
      }
    ]);
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
