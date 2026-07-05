import { afterEach, describe, expect, it, vi } from "vitest";

import {
  PROJECT_ORDER_STORAGE_KEY,
  PROJECT_ROOTS_STORAGE_KEY,
  SELECTED_PROJECT_ROOT_STORAGE_KEY,
  STANDALONE_PROJECT_SENTINEL
} from "./projectStorage";
import { SETTINGS_SIDEBAR_WIDTH_STORAGE_KEY } from "./paneLayout";

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

  it("persists settings sidebar width from the shared store", async () => {
    stubMemoryLocalStorage({
      [SETTINGS_SIDEBAR_WIDTH_STORAGE_KEY]: "260"
    });

    const { useAppUiStore } = await import("./appUiStore");
    const store = useAppUiStore.getState();

    expect(store.settingsSidebarWidth).toBe(260);

    store.setSettingsSidebarWidth(300);

    expect(localStorage.getItem(SETTINGS_SIDEBAR_WIDTH_STORAGE_KEY)).toBe("300");
    expect(useAppUiStore.getState().settingsSidebarWidth).toBe(300);
  });

  it("keeps permission mode in the shared store", async () => {
    stubMemoryLocalStorage();

    const { useAppUiStore } = await import("./appUiStore");

    expect(useAppUiStore.getState().permissionMode).toBe("default");

    useAppUiStore.getState().setPermissionMode("accept-edits");

    expect(useAppUiStore.getState().permissionMode).toBe("accept-edits");
  });

  it("keeps turn runtime state in the shared store", async () => {
    stubMemoryLocalStorage();

    const { useAppUiStore } = await import("./appUiStore");
    const store = useAppUiStore.getState();

    expect(store.isProcessing).toBe(false);
    expect(store.currentTurnId).toBeNull();
    expect(store.messageQueue).toEqual([]);
    expect(store.pendingUserQuestion).toBeNull();
    expect(store.usageSnapshot).toBeNull();

    store.setIsProcessing(true);
    store.setCurrentTurnId("turn-1");
    store.setMessageQueue((current) => [
      ...current,
      { content: "queued", images: [] }
    ]);
    store.setPendingUserQuestion({
      sessionId: "session-1",
      turnId: "turn-1",
      question: "continue?",
    });
    store.setUsageSnapshot((current) => ({
      ...current,
      inputTokens: 10,
      outputTokens: 5,
    }));

    expect(useAppUiStore.getState().isProcessing).toBe(true);
    expect(useAppUiStore.getState().currentTurnId).toBe("turn-1");
    expect(useAppUiStore.getState().messageQueue).toEqual([
      { content: "queued", images: [] }
    ]);
    expect(useAppUiStore.getState().pendingUserQuestion?.question).toBe("continue?");
    expect(useAppUiStore.getState().usageSnapshot).toEqual({
      inputTokens: 10,
      outputTokens: 5,
    });

    useAppUiStore.getState().clearTurnState();

    expect(useAppUiStore.getState().isProcessing).toBe(false);
    expect(useAppUiStore.getState().currentTurnId).toBeNull();
    expect(useAppUiStore.getState().messageQueue).toEqual([]);
    expect(useAppUiStore.getState().pendingUserQuestion).toBeNull();
    expect(useAppUiStore.getState().usageSnapshot).toEqual({
      inputTokens: 10,
      outputTokens: 5,
    });
  });

  it("keeps composer draft and attachments in the shared store", async () => {
    stubMemoryLocalStorage();

    const { useAppUiStore } = await import("./appUiStore");
    const store = useAppUiStore.getState();

    expect(store.draft).toBe("");
    expect(store.composerImages).toEqual([]);

    store.setDraft("hello");
    store.setComposerImages([
      {
        id: "image-1",
        name: "screenshot.png",
        mediaType: "image/png",
        base64: "abc",
        dataUrl: "data:image/png;base64,abc",
        size: 3,
      }
    ]);

    expect(useAppUiStore.getState().draft).toBe("hello");
    expect(useAppUiStore.getState().composerImages).toEqual([
      {
        id: "image-1",
        name: "screenshot.png",
        mediaType: "image/png",
        base64: "abc",
        dataUrl: "data:image/png;base64,abc",
        size: 3,
      }
    ]);

    store.setComposerImages((current) => current.filter((image) => image.id !== "image-1"));

    expect(useAppUiStore.getState().composerImages).toEqual([]);
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
