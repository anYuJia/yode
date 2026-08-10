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

  it("keeps turn, composer, timeline, and usage state isolated per session", async () => {
    stubMemoryLocalStorage();

    const { useAppUiStore } = await import("./appUiStore");
    const store = useAppUiStore.getState();
    const sessionOneImage = {
      id: "session-one-image",
      name: "one.png",
      mediaType: "image/png",
      base64: "one",
      dataUrl: "data:image/png;base64,one",
      size: 3
    };

    store.setActiveSessionId("session-1");
    store.setDraft("会话一草稿");
    store.setComposerImages([sessionOneImage]);
    store.setCurrentTurnId("turn-1");
    store.setIsProcessing(true);
    store.setMessageQueue([{ content: "会话一排队消息", images: [] }]);
    store.setPendingUserQuestion({ sessionId: "session-1", turnId: "turn-1", question: "继续吗？" });
    store.setTimelineItems([{ id: "one", kind: "assistant", title: "助手", body: "会话一内容" }]);
    store.setUsageSnapshot({ inputTokens: 10, outputTokens: 2 });

    store.setActiveSessionId("session-2");
    expect(useAppUiStore.getState().draft).toBe("");
    expect(useAppUiStore.getState().composerImages).toEqual([]);
    expect(useAppUiStore.getState().isProcessing).toBe(false);
    expect(useAppUiStore.getState().timelineItems).toEqual([]);

    store.setDraft("会话二草稿");
    store.setTimelineItems([{ id: "two", kind: "assistant", title: "助手", body: "会话二内容" }]);
    store.setUsageSnapshot({ inputTokens: 3 });

    store.setActiveSessionId("session-1");
    const restored = useAppUiStore.getState();
    expect(restored.draft).toBe("会话一草稿");
    expect(restored.composerImages).toEqual([sessionOneImage]);
    expect(restored.currentTurnId).toBe("turn-1");
    expect(restored.isProcessing).toBe(true);
    expect(restored.messageQueue).toEqual([{ content: "会话一排队消息", images: [] }]);
    expect(restored.pendingUserQuestion?.question).toBe("继续吗？");
    expect(restored.timelineItems).toMatchObject([{ id: "one", body: "会话一内容" }]);
    expect(restored.usageSnapshot).toEqual({ inputTokens: 10, outputTokens: 2 });

    store.setActiveSessionId("session-2");
    expect(useAppUiStore.getState().draft).toBe("会话二草稿");
    expect(useAppUiStore.getState().timelineItems).toMatchObject([{ id: "two", body: "会话二内容" }]);
    expect(useAppUiStore.getState().usageSnapshot).toEqual({ inputTokens: 3 });
  });

  it("removes an inactive session snapshot so attachments and queued messages can be collected", async () => {
    stubMemoryLocalStorage();

    const { useAppUiStore } = await import("./appUiStore");
    const store = useAppUiStore.getState();
    store.setActiveSessionId("session-to-remove");
    store.setTimelineItems([{ id: "large", kind: "assistant", title: "助手", body: "历史" }]);
    store.setMessageQueue([{ content: "不应继续发送", images: [] }]);

    expect(useAppUiStore.getState().sessionUiStates["session-to-remove"]).toBeDefined();
    store.removeSessionUiState("session-to-remove");

    expect(useAppUiStore.getState().sessionUiStates["session-to-remove"]).toBeUndefined();
    expect(store.getSessionUiState("session-to-remove")).toEqual({
      composerImages: [],
      currentTurnId: null,
      draft: "",
      isProcessing: false,
      messageQueue: [],
      pendingUserQuestion: null,
      timelineItems: [],
      usageSnapshot: null,
      hasMoreHistory: false,
      historyLoading: false,
      historyError: false,
      historyCursor: null
    });
  });

  it("moves detached review input out of the source session before restoring a failed review", async () => {
    stubMemoryLocalStorage();

    const { useAppUiStore } = await import("./appUiStore");
    const store = useAppUiStore.getState();
    const reviewImages = [{
      id: "review-image",
      name: "review.png",
      mediaType: "image/png",
      base64: "review",
      dataUrl: "data:image/png;base64,review",
      size: 6
    }];

    store.setActiveSessionId("session-source");
    store.setDraft("/review 检查这个改动");
    store.setComposerImages(reviewImages);

    // detached /review：必须先清空原会话，再进入独立草稿槽。
    store.setDraft("", "session-source");
    store.setComposerImages([], "session-source");
    store.setActiveSessionId(null);
    store.setDraft("");
    store.setComposerImages([]);

    // 请求失败时只在独立草稿槽恢复，不能写回原会话。
    store.setDraft("/review 检查这个改动", null);
    store.setComposerImages(reviewImages, null);

    expect(store.getSessionUiState("session-source")).toMatchObject({
      draft: "",
      composerImages: []
    });
    expect(useAppUiStore.getState()).toMatchObject({
      activeSessionId: null,
      draft: "/review 检查这个改动",
      composerImages: reviewImages
    });
  });

  it("keeps a newly created turn inactive when its draft request no longer owns the screen", async () => {
    stubMemoryLocalStorage();

    const { canActivateCreatedSession, useAppUiStore } = await import("./appUiStore");
    const store = useAppUiStore.getState();

    store.setActiveSessionId(null);
    store.setDraft("新会话请求");
    store.setTimelineItems([{ id: "pending", kind: "user", title: "用户", body: "新会话请求" }]);
    store.setActiveSessionId("session-b");

    // 回包始终提升草稿状态到服务端新建的会话；但用户已切到 B，不得抢回焦点。
    store.promoteDraftToSession("session-created");
    expect(canActivateCreatedSession("session-b", 7, 7)).toBe(false);
    expect(useAppUiStore.getState().activeSessionId).toBe("session-b");
    expect(store.getSessionUiState("session-created")).toMatchObject({
      draft: "新会话请求",
      timelineItems: [{ id: "pending", body: "新会话请求" }]
    });
    expect(canActivateCreatedSession(null, 7, 7)).toBe(true);
    expect(canActivateCreatedSession(null, 7, 8)).toBe(false);
  });

  it("keeps session and timeline state in the shared store", async () => {
    stubMemoryLocalStorage();

    const { useAppUiStore } = await import("./appUiStore");
    const store = useAppUiStore.getState();

    expect(store.activeSessionId).toBeNull();
    expect(store.sessionItems).toEqual([]);
    expect(store.timelineItems).toEqual([]);

    store.setActiveSessionId("session-1");
    store.setSessionItems([
      {
        id: "session-1",
        title: "会话",
        provider: "openai",
        model: "gpt-5",
        updatedAt: "2026-07-05T12:00:00.000Z",
        active: true,
      }
    ]);
    store.setTimelineItems((current) => [
      ...current,
      {
        id: "item-1",
        kind: "assistant",
        title: "助手",
        body: "完成",
      }
    ]);

    expect(useAppUiStore.getState().activeSessionId).toBe("session-1");
    expect(useAppUiStore.getState().sessionItems).toHaveLength(1);
    expect(useAppUiStore.getState().timelineItems).toHaveLength(1);
  });

  it("keeps bootstrap state in the shared store", async () => {
    stubMemoryLocalStorage();

    const { useAppUiStore } = await import("./appUiStore");
    const store = useAppUiStore.getState();

    expect(store.bootstrap.workspacePath).toBe("");

    store.setBootstrap((current) => ({
      ...current,
      provider: "anthropic",
      model: "claude-sonnet-4",
      permissionMode: "accept-edits",
      sessions: [
        {
          id: "session-1",
          title: "会话",
          provider: "anthropic",
          model: "claude-sonnet-4",
          updatedAt: "2026-07-05T12:00:00.000Z",
        }
      ],
    }));

    expect(useAppUiStore.getState().bootstrap.provider).toBe("anthropic");
    expect(useAppUiStore.getState().bootstrap.model).toBe("claude-sonnet-4");
    expect(useAppUiStore.getState().bootstrap.permissionMode).toBe("accept-edits");
    expect(useAppUiStore.getState().bootstrap.sessions).toHaveLength(1);
  });

  it("keeps pane drag state in the shared store", async () => {
    stubMemoryLocalStorage();

    const { useAppUiStore } = await import("./appUiStore");

    expect(useAppUiStore.getState().draggingPane).toBeNull();

    useAppUiStore.getState().setDraggingPane("inspector");

    expect(useAppUiStore.getState().draggingPane).toBe("inspector");

    useAppUiStore.getState().setDraggingPane(null);

    expect(useAppUiStore.getState().draggingPane).toBeNull();
  });

  it("migrates legacy selected-project-root and empty settings tab on load", async () => {
    stubMemoryLocalStorage({
      [SELECTED_PROJECT_ROOT_STORAGE_KEY]: "null",
      "yode-active-tab": ""
    });

    const {
      ACTIVE_SETTINGS_TAB_STORAGE_KEY,
      DEFAULT_SETTINGS_TAB,
      loadActiveSettingsTab,
      useAppUiStore
    } = await import("./appUiStore");

    expect(useAppUiStore.getState().selectedProjectRoot).toBeNull();
    expect(localStorage.getItem(SELECTED_PROJECT_ROOT_STORAGE_KEY)).toBe(STANDALONE_PROJECT_SENTINEL);
    expect(loadActiveSettingsTab()).toBe(DEFAULT_SETTINGS_TAB);
    expect(localStorage.getItem(ACTIVE_SETTINGS_TAB_STORAGE_KEY)).toBe(DEFAULT_SETTINGS_TAB);
  });

  it("tracks history paging state per session in the store", async () => {
    stubMemoryLocalStorage();

    const { useAppUiStore } = await import("./appUiStore");
    const store = useAppUiStore.getState();
    store.setActiveSessionId("history-session");

    expect(store.getSessionUiState("history-session").hasMoreHistory).toBe(false);
    store.setHasMoreHistory(true, "history-session");
    store.setHistoryLoading(true, "history-session");
    store.setHistoryCursor(42, "history-session");
    const ui = store.getSessionUiState("history-session");
    expect(ui.hasMoreHistory).toBe(true);
    expect(ui.historyLoading).toBe(true);
    expect(ui.historyCursor).toBe(42);

    // 游标按会话隔离
    store.setActiveSessionId("other-session");
    expect(store.getSessionUiState("other-session").historyCursor).toBeNull();
  });

  it("keeps replay state machine in the store with retry generation", async () => {
    stubMemoryLocalStorage();

    const { useAppUiStore } = await import("./appUiStore");
    const store = useAppUiStore.getState();
    expect(store.replayState.status).toBe("idle");
    expect(store.replayState.retryGeneration).toBe(0);

    store.setReplayState({ status: "error", error: "查询失败", retryGeneration: 0 });
    expect(useAppUiStore.getState().replayState.status).toBe("error");

    store.retryReplay();
    const after = useAppUiStore.getState().replayState;
    expect(after.status).toBe("idle");
    expect(after.error).toBeUndefined();
    expect(after.retryGeneration).toBe(1);
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
