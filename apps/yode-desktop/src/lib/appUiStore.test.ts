import { afterEach, describe, expect, it, vi } from "vitest";

import {
  PROJECT_ORDER_STORAGE_KEY,
  PROJECT_ROOTS_STORAGE_KEY,
  SELECTED_PROJECT_ROOT_STORAGE_KEY,
  STANDALONE_PROJECT_SENTINEL
} from "./projectStorage";

describe("app UI store", () => {
  afterEach(() => {
    vi.resetModules();
    vi.unstubAllGlobals();
  });

  it("persists project state from store setters", async () => {
    stubMemoryLocalStorage({
      [PROJECT_ROOTS_STORAGE_KEY]: JSON.stringify(["/repo-a"]),
      [PROJECT_ORDER_STORAGE_KEY]: JSON.stringify(["/repo-a"]),
    });

    const { useAppUiStore } = await import("./appUiStore");
    const store = useAppUiStore.getState();

    expect(store.projectRoots).toEqual(["/repo-a"]);
    expect(store.projectOrder).toEqual(["/repo-a"]);

    store.setProjectRoots((current) => [...current, "/repo-b"]);
    store.setProjectOrder(["/repo-b", "/repo-a"]);
    store.setSelectedProjectRoot(null);

    expect(JSON.parse(localStorage.getItem(PROJECT_ROOTS_STORAGE_KEY) || "[]")).toEqual([
      "/repo-a",
      "/repo-b",
    ]);
    expect(JSON.parse(localStorage.getItem(PROJECT_ORDER_STORAGE_KEY) || "[]")).toEqual([
      "/repo-b",
      "/repo-a",
    ]);
    expect(localStorage.getItem(SELECTED_PROJECT_ROOT_STORAGE_KEY)).toBe(STANDALONE_PROJECT_SENTINEL);
  });

  it("loads and saves the active settings tab through shared helpers", async () => {
    stubMemoryLocalStorage();

    const {
      ACTIVE_SETTINGS_TAB_STORAGE_KEY,
      KEYBOARD_SHORTCUTS_SETTINGS_TAB,
      loadActiveSettingsTab,
      saveActiveSettingsTab
    } = await import("./appUiStore");

    expect(loadActiveSettingsTab()).toBe("常规");
    expect(saveActiveSettingsTab(KEYBOARD_SHORTCUTS_SETTINGS_TAB)).toBe(KEYBOARD_SHORTCUTS_SETTINGS_TAB);
    expect(localStorage.getItem(ACTIVE_SETTINGS_TAB_STORAGE_KEY)).toBe(KEYBOARD_SHORTCUTS_SETTINGS_TAB);
    expect(loadActiveSettingsTab()).toBe(KEYBOARD_SHORTCUTS_SETTINGS_TAB);
  });
});

function stubMemoryLocalStorage(seed: Record<string, string> = {}) {
  const values = new Map(Object.entries(seed));
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
    removeItem: (key: string) => {
      values.delete(key);
    },
  });
}
