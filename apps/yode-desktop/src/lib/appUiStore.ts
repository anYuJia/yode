import { create } from "zustand";

import { fallbackBootstrap } from "./desktopTypes";
import type {
  Bootstrap,
  ImageAttachment,
  PendingUserQuestion,
  RunState,
  SessionSummary,
  TimelineItem,
  UsageSnapshot,
  ViewMode
} from "./desktopTypes";
import {
  loadAppLanguage,
  normalizeAppLanguage
} from "./appearanceSettings";
import {
  applyGeneralSettings,
  loadGeneralSettings,
  type GeneralSettings
} from "./desktopSettings";
import {
  INSPECTOR_WIDTH_STORAGE_KEY,
  loadInitialPaneSize,
  type PaneKind,
  SETTINGS_SIDEBAR_WIDTH_STORAGE_KEY,
  SIDEBAR_WIDTH_STORAGE_KEY,
  TERMINAL_HEIGHT_STORAGE_KEY
} from "./paneLayout";
import {
  loadStoredProjectOrder,
  loadStoredProjectRoots,
  loadStoredSelectedProjectRoot,
  PROJECT_ORDER_STORAGE_KEY,
  PROJECT_ROOTS_STORAGE_KEY,
  SELECTED_PROJECT_ROOT_STORAGE_KEY,
  STANDALONE_PROJECT_SENTINEL
} from "./projectStorage";
import {
  runStorageMigrations,
  storageReadString,
  storageWriteJson,
  storageWriteString,
  type StorageMigration
} from "./storageAdapter";

export const ACTIVE_SETTINGS_TAB_STORAGE_KEY = "yode-active-tab";
export const DEFAULT_SETTINGS_TAB = "常规";
export const KEYBOARD_SHORTCUTS_SETTINGS_TAB = "键盘快捷键";

/**
 * 历史键值迁移：旧版本把未选中项目写成字符串 "null"/"undefined"，
 * 新版本统一使用 STANDALONE_PROJECT_SENTINEL；空串设置标签回退默认值。
 * 模块加载时执行一次（幂等，已迁移的 key 不再重复处理）。
 */
const UI_STORAGE_MIGRATIONS: StorageMigration[] = [
  {
    key: SELECTED_PROJECT_ROOT_STORAGE_KEY,
    migrate: (raw) => {
      if (raw === "null" || raw === "undefined") {
        return STANDALONE_PROJECT_SENTINEL;
      }
      return undefined;
    }
  },
  {
    key: ACTIVE_SETTINGS_TAB_STORAGE_KEY,
    migrate: (raw) => (raw === "" ? DEFAULT_SETTINGS_TAB : undefined)
  }
];

if (typeof localStorage !== "undefined") {
  runStorageMigrations(UI_STORAGE_MIGRATIONS);
}

type StateUpdater<T> = T | ((current: T) => T);

export type QueuedComposerMessage = {
  content: string;
  images: ImageAttachment[];
};

/** 会话专属的易失 UI 状态。未绑定会话的新对话使用独立的草稿槽。 */
export type SessionUiState = {
  composerImages: ImageAttachment[];
  currentTurnId: string | null;
  draft: string;
  isProcessing: boolean;
  messageQueue: QueuedComposerMessage[];
  pendingUserQuestion: PendingUserQuestion | null;
  timelineItems: TimelineItem[];
  usageSnapshot: UsageSnapshot | null;
  /** 历史消息分页：是否还有更早消息、加载中/失败标记。 */
  hasMoreHistory: boolean;
  historyLoading: boolean;
  historyError: boolean;
  /** 已加载窗口中最旧消息的 sort_order 游标；无更早消息时为 null。 */
  historyCursor: number | null;
};

/**
 * 断线恢复状态机：idle → loading → done/error。
 * error 时保留当前锁定状态，并提供 retryReplay 可重试路径。
 */
export type ReplayState = {
  status: "idle" | "loading" | "done" | "error";
  error?: string;
  /** 重试代数：调用 retryReplay 递增，触发重放监听者重新执行。 */
  retryGeneration: number;
};

const DRAFT_SESSION_KEY = "__draft__";

function sessionUiStateKey(sessionId: string | null) {
  return sessionId ?? DRAFT_SESSION_KEY;
}

/** 仅当用户仍处于发起请求的草稿槽时，才允许回包切换到新建会话。 */
export function canActivateCreatedSession(
  activeSessionId: string | null,
  requestSequence: number | null,
  currentRequestSequence: number
) {
  return activeSessionId === null && requestSequence !== null && requestSequence === currentRequestSequence;
}

function emptySessionUiState(): SessionUiState {
  return {
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
  };
}

type AppUiState = {
  activeSessionId: string | null;
  appLang: string;
  bootstrap: Bootstrap;
  composerImages: ImageAttachment[];
  currentTurnId: string | null;
  draft: string;
  draggingPane: PaneKind | null;
  generalSettings: GeneralSettings;
  inspectorOpen: boolean;
  inspectorWidth: number;
  isProcessing: boolean;
  messageQueue: QueuedComposerMessage[];
  pendingUserQuestion: PendingUserQuestion | null;
  permissionMode: string;
  projectOrder: string[];
  projectRoots: string[];
  /** 后端持久化 turn journal 的桌面投影（bootstrap.runs 与轮询刷新共用）。 */
  runs: RunState[];
  /** 断线恢复：事件重放状态。error 时保留锁定状态并可重试。 */
  replayState: ReplayState;
  selectedProjectRoot: string | null | undefined;
  sessionUiStates: Record<string, SessionUiState>;
  sessionItems: SessionSummary[];
  settingsSidebarWidth: number;
  sidebarOpen: boolean;
  sidebarWidth: number;
  terminalHeight: number;
  terminalOpenByConversation: Record<string, boolean>;
  timelineItems: TimelineItem[];
  usageSnapshot: UsageSnapshot | null;
  viewMode: ViewMode;
  clearTurnState: () => void;
  getSessionUiState: (sessionId: string | null) => SessionUiState;
  removeSessionUiState: (sessionId: string) => void;
  promoteDraftToSession: (sessionId: string) => void;
  reloadProjectStorage: () => void;
  refreshGeneralSettings: (options?: { apply?: boolean }) => void;
  retryReplay: () => void;
  setActiveSessionId: (sessionId: string | null) => void;
  setAppLang: (lang: string) => void;
  setBootstrap: (bootstrap: StateUpdater<Bootstrap>) => void;
  setComposerImages: (images: StateUpdater<ImageAttachment[]>, sessionId?: string | null) => void;
  setCurrentTurnId: (turnId: StateUpdater<string | null>, sessionId?: string | null) => void;
  setDraft: (draft: string, sessionId?: string | null) => void;
  setDraggingPane: (pane: PaneKind | null) => void;
  setHistoryLoading: (loading: boolean, sessionId?: string | null) => void;
  setHasMoreHistory: (hasMore: boolean, sessionId?: string | null) => void;
  setHistoryError: (error: boolean, sessionId?: string | null) => void;
  setHistoryCursor: (cursor: number | null, sessionId?: string | null) => void;
  setInspectorOpen: (open: boolean) => void;
  setInspectorWidth: (width: number) => void;
  setIsProcessing: (isProcessing: boolean, sessionId?: string | null) => void;
  setMessageQueue: (queue: StateUpdater<QueuedComposerMessage[]>, sessionId?: string | null) => void;
  setPendingUserQuestion: (question: StateUpdater<PendingUserQuestion | null>, sessionId?: string | null) => void;
  setPermissionMode: (mode: string) => void;
  setProjectOrder: (order: StateUpdater<string[]>) => void;
  setProjectRoots: (roots: StateUpdater<string[]>) => void;
  setReplayState: (state: StateUpdater<ReplayState>) => void;
  setRuns: (runs: StateUpdater<RunState[]>) => void;
  setSelectedProjectRoot: (root: StateUpdater<string | null | undefined>) => void;
  setSessionItems: (sessions: StateUpdater<SessionSummary[]>) => void;
  setSettingsSidebarWidth: (width: number) => void;
  setSidebarOpen: (open: boolean) => void;
  setSidebarWidth: (width: number) => void;
  setTerminalHeight: (height: number) => void;
  setTerminalOpenForConversation: (conversationKey: string, open: boolean) => void;
  setTimelineItems: (items: StateUpdater<TimelineItem[]>, sessionId?: string | null) => void;
  setUsageSnapshot: (snapshot: StateUpdater<UsageSnapshot | null>, sessionId?: string | null) => void;
  updateSessionUiState: (
    sessionId: string | null | undefined,
    updater: (current: SessionUiState) => SessionUiState
  ) => void;
  setViewMode: (mode: ViewMode) => void;
};

function storedViewMode(): ViewMode {
  const raw = storageReadString("yode-view-mode", "chat");
  return raw === "settings" ? "settings" : "chat";
}

export function loadActiveSettingsTab() {
  return storageReadString(ACTIVE_SETTINGS_TAB_STORAGE_KEY, DEFAULT_SETTINGS_TAB);
}

export function saveActiveSettingsTab(tab: string) {
  storageWriteString(ACTIVE_SETTINGS_TAB_STORAGE_KEY, tab);
  return tab;
}

function resolveUpdater<T>(updater: StateUpdater<T>, current: T): T {
  return typeof updater === "function"
    ? (updater as (current: T) => T)(current)
    : updater;
}

function stateForSession(state: AppUiState, sessionId: string | null): SessionUiState {
  return state.sessionUiStates[sessionUiStateKey(sessionId)] ?? emptySessionUiState();
}

export const useAppUiStore = create<AppUiState>((set, get) => ({
  activeSessionId: null,
  appLang: loadAppLanguage(),
  bootstrap: fallbackBootstrap,
  composerImages: [],
  currentTurnId: null,
  draft: "",
  draggingPane: null,
  generalSettings: loadGeneralSettings(),
  inspectorOpen: true,
  inspectorWidth: loadInitialPaneSize("inspector", INSPECTOR_WIDTH_STORAGE_KEY),
  isProcessing: false,
  messageQueue: [],
  pendingUserQuestion: null,
  permissionMode: "default",
  projectOrder: loadStoredProjectOrder(),
  projectRoots: loadStoredProjectRoots(),
  runs: [],
  replayState: { status: "idle", retryGeneration: 0 },
  selectedProjectRoot: loadStoredSelectedProjectRoot(),
  sessionUiStates: {},
  sessionItems: [],
  settingsSidebarWidth: loadInitialPaneSize("settingsSidebar", SETTINGS_SIDEBAR_WIDTH_STORAGE_KEY),
  sidebarOpen: true,
  sidebarWidth: loadInitialPaneSize("sidebar", SIDEBAR_WIDTH_STORAGE_KEY),
  terminalHeight: loadInitialPaneSize("terminal", TERMINAL_HEIGHT_STORAGE_KEY),
  terminalOpenByConversation: {},
  timelineItems: [],
  usageSnapshot: null,
  viewMode: storedViewMode(),
  clearTurnState: () => get().updateSessionUiState(undefined, (current) => ({
    ...current,
    currentTurnId: null,
    isProcessing: false,
    messageQueue: [],
    pendingUserQuestion: null
  })),
  getSessionUiState: (sessionId) => stateForSession(get(), sessionId),
  removeSessionUiState: (sessionId) => set((state) => {
    const key = sessionUiStateKey(sessionId);
    if (!(key in state.sessionUiStates)) return state;
    const sessionUiStates = { ...state.sessionUiStates };
    delete sessionUiStates[key];
    return { sessionUiStates };
  }),
  promoteDraftToSession: (sessionId) => set((state) => {
    const draftSessionState = stateForSession(state, null);
    return {
      sessionUiStates: {
        ...state.sessionUiStates,
        [sessionUiStateKey(null)]: emptySessionUiState(),
        [sessionUiStateKey(sessionId)]: draftSessionState
      },
      ...(state.activeSessionId === null ? emptySessionUiState() : {})
    };
  }),
  reloadProjectStorage: () => set({
    projectOrder: loadStoredProjectOrder(),
    projectRoots: loadStoredProjectRoots(),
    selectedProjectRoot: loadStoredSelectedProjectRoot(),
  }),
  retryReplay: () => set((state) => ({
    replayState: {
      ...state.replayState,
      status: "idle",
      error: undefined,
      retryGeneration: state.replayState.retryGeneration + 1
    }
  })),
  refreshGeneralSettings: (options) => {
    set({ generalSettings: loadGeneralSettings() });
    if (options?.apply !== false) {
      void applyGeneralSettings();
    }
  },
  setActiveSessionId: (activeSessionId) => set((state) => {
    const key = sessionUiStateKey(activeSessionId);
    const sessionUiState = stateForSession(state, activeSessionId);
    return {
      activeSessionId,
      ...sessionUiState,
      sessionUiStates: state.sessionUiStates[key]
        ? state.sessionUiStates
        : { ...state.sessionUiStates, [key]: sessionUiState }
    };
  }),
  setAppLang: (appLang) => {
    set({ appLang: normalizeAppLanguage(appLang) });
  },
  setBootstrap: (updater) => {
    const bootstrap = resolveUpdater(updater, get().bootstrap);
    set({ bootstrap });
  },
  setComposerImages: (updater, sessionId) => {
    get().updateSessionUiState(sessionId, (current) => ({
      ...current,
      composerImages: resolveUpdater(updater, current.composerImages)
    }));
  },
  setCurrentTurnId: (updater, sessionId) => {
    get().updateSessionUiState(sessionId, (current) => ({
      ...current,
      currentTurnId: resolveUpdater(updater, current.currentTurnId)
    }));
  },
  setDraft: (draft, sessionId) =>
    get().updateSessionUiState(sessionId, (current) => ({ ...current, draft })),
  setDraggingPane: (draggingPane) => set({ draggingPane }),
  setHistoryLoading: (historyLoading, sessionId) => {
    get().updateSessionUiState(sessionId, (current) => ({ ...current, historyLoading }));
  },
  setHasMoreHistory: (hasMoreHistory, sessionId) => {
    get().updateSessionUiState(sessionId, (current) => ({ ...current, hasMoreHistory }));
  },
  setHistoryError: (historyError, sessionId) => {
    get().updateSessionUiState(sessionId, (current) => ({ ...current, historyError }));
  },
  setHistoryCursor: (historyCursor, sessionId) => {
    get().updateSessionUiState(sessionId, (current) => ({ ...current, historyCursor }));
  },
  setInspectorOpen: (inspectorOpen) => set({ inspectorOpen }),
  setInspectorWidth: (inspectorWidth) => {
    // 拖动过程中不写 localStorage（由 pointerup 时一次性落盘），
    // 避免每个 pointermove 都同步写盘
    if (!get().draggingPane) {
      storageWriteString(INSPECTOR_WIDTH_STORAGE_KEY, String(inspectorWidth));
    }
    set({ inspectorWidth });
  },
  setIsProcessing: (isProcessing, sessionId) => {
    get().updateSessionUiState(sessionId, (current) => ({ ...current, isProcessing }));
  },
  setMessageQueue: (updater, sessionId) => {
    get().updateSessionUiState(sessionId, (current) => ({
      ...current,
      messageQueue: resolveUpdater(updater, current.messageQueue)
    }));
  },
  setPendingUserQuestion: (updater, sessionId) => {
    get().updateSessionUiState(sessionId, (current) => ({
      ...current,
      pendingUserQuestion: resolveUpdater(updater, current.pendingUserQuestion)
    }));
  },
  setPermissionMode: (permissionMode) => set({ permissionMode }),
  setReplayState: (updater) => {
    const replayState = resolveUpdater(updater, get().replayState);
    set({ replayState });
  },
  setRuns: (updater) => {
    const runs = resolveUpdater(updater, get().runs);
    set({ runs });
  },
  setProjectOrder: (updater) => {
    const projectOrder = resolveUpdater(updater, get().projectOrder);
    storageWriteJson(PROJECT_ORDER_STORAGE_KEY, projectOrder);
    set({ projectOrder });
  },
  setProjectRoots: (updater) => {
    const projectRoots = resolveUpdater(updater, get().projectRoots);
    storageWriteJson(PROJECT_ROOTS_STORAGE_KEY, projectRoots);
    set({ projectRoots });
  },
  setSelectedProjectRoot: (updater) => {
    const selectedProjectRoot = resolveUpdater(updater, get().selectedProjectRoot);
    if (selectedProjectRoot !== undefined) {
      storageWriteString(
        SELECTED_PROJECT_ROOT_STORAGE_KEY,
        selectedProjectRoot === null ? STANDALONE_PROJECT_SENTINEL : selectedProjectRoot
      );
    }
    set({ selectedProjectRoot });
  },
  setSessionItems: (updater) => {
    const sessionItems = resolveUpdater(updater, get().sessionItems);
    set({ sessionItems });
  },
  setSettingsSidebarWidth: (settingsSidebarWidth) => {
    storageWriteString(SETTINGS_SIDEBAR_WIDTH_STORAGE_KEY, String(settingsSidebarWidth));
    set({ settingsSidebarWidth });
  },
  setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
  setSidebarWidth: (sidebarWidth) => {
    if (!get().draggingPane) {
      storageWriteString(SIDEBAR_WIDTH_STORAGE_KEY, String(sidebarWidth));
    }
    set({ sidebarWidth });
  },
  setTerminalHeight: (terminalHeight) => {
    if (!get().draggingPane) {
      storageWriteString(TERMINAL_HEIGHT_STORAGE_KEY, String(terminalHeight));
    }
    set({ terminalHeight });
  },
  setTerminalOpenForConversation: (conversationKey, open) => set((state) => ({
    terminalOpenByConversation: {
      ...state.terminalOpenByConversation,
      [conversationKey]: open
    }
  })),
  setTimelineItems: (updater, sessionId) => {
    get().updateSessionUiState(sessionId, (current) => ({
      ...current,
      timelineItems: resolveUpdater(updater, current.timelineItems)
    }));
  },
  setUsageSnapshot: (updater, sessionId) => {
    get().updateSessionUiState(sessionId, (current) => ({
      ...current,
      usageSnapshot: resolveUpdater(updater, current.usageSnapshot)
    }));
  },
  updateSessionUiState: (sessionId, updater) => set((state) => {
    const targetSessionId = sessionId === undefined ? state.activeSessionId : sessionId;
    const key = sessionUiStateKey(targetSessionId);
    const sessionUiState = updater(stateForSession(state, targetSessionId));
    return {
      sessionUiStates: {
        ...state.sessionUiStates,
        [key]: sessionUiState
      },
      ...(state.activeSessionId === targetSessionId ? sessionUiState : {})
    };
  }),
  setViewMode: (viewMode) => {
    storageWriteString("yode-view-mode", viewMode);
    set({ viewMode });
  }
}));
