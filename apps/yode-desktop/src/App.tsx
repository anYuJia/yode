import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Archive,
  Bot,
  ChevronDown,
  ChevronRight,
  CircleDot,
  Clock3,
  Code2,
  Command,
  FileCode2,
  Folder,
  GitBranch,
  Hammer,
  History,
  KeyRound,
  MessageSquarePlus,
  MoreHorizontal,
  Paperclip,
  Pause,
  FolderPlus,
  Plus,
  Search,
  Send,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  TerminalSquare,
  Workflow,
  PanelRight,
  PanelRightClose,
  X,
  Sun,
  Moon,
  Monitor,
  Copy,
  Download,
  Hand,
  Shield,
  AlertCircle,
  Check,
  Pin,
  Trash2,
  Square
} from "lucide-react";
import React, { useCallback, useEffect, useLayoutEffect, useMemo, useState, useRef } from "react";
import { createPortal } from "react-dom";
import "./lib/highlightLanguages";

import {
  Bootstrap,
  DesktopEvent,
  DesktopMessage,
  fallbackBootstrap,
  SessionSummary,
  TimelineItem,
  TurnAccepted,
  ImageAttachment,
  RunState,
  UsageSnapshot,
  ViewMode
} from "./lib/desktopTypes";
import { SettingsShell } from "./components/SettingsShell";
import { TerminalDrawer } from "./components/TerminalDrawer";
import { PROVIDERS_META } from "./components/settings/ProvidersSettings";
import { Sidebar } from "./components/Sidebar";
import { Topbar } from "./components/Topbar";
import { ChatWorkspace } from "./components/ChatWorkspace";
import {
  messagesToTimelineItems,
  upsertActiveSession,
  deriveSessionTitle,
  projectLabelFromPath,
  ConversationTurn
} from "./lib/timelineUtils";
import { KEYBOARD_SHORTCUTS_CHANGE_EVENT, findShortcutAction, loadShortcutBindings } from "./lib/keyboardShortcuts";
import {
  executeLocalSlashCommand,
  formatUsageSnapshot
} from "./lib/localSlashCommands";
import {
  discardPendingDeltas,
  handleDesktopRuntimeEvent,
  isTerminalTurnEvent,
  resetTurnTracksForSession
} from "./lib/desktopEventHandlers";
import { startCancellationWatchdog } from "./lib/cancellationWatchdog";
import { CancellationWatchdogRegistry } from "./lib/cancellationWatchdogRegistry";
import { GENERAL_SETTINGS_CHANGE_EVENT, toggleBottomPanelSetting } from "./lib/desktopSettings";
import {
  PROJECT_ROOTS_CHANGED_EVENT,
  SESSION_DELETED_PERMANENTLY_EVENT,
  SESSION_UNARCHIVED_EVENT,
  SESSIONS_IMPORTED_EVENT,
  archiveSessionLocally,
  dedupeProjectRoots,
  detailFromSessionIdEvent,
  normalizeProjectRoot,
  visibleSessions
} from "./lib/projectStorage";
import {
  computePaneDragSize,
  isPaneCollapsed,
  PaneDragState,
  PaneKind,
} from "./lib/paneLayout";
import {
  INSPECTOR_WIDTH_STORAGE_KEY,
  SIDEBAR_WIDTH_STORAGE_KEY,
  TERMINAL_HEIGHT_STORAGE_KEY
} from "./lib/paneLayout";
import {
  DEFAULT_LLM_CHANGE_EVENT,
  detailFromDefaultLlmChangeEvent,
  preferredModelFromStorage,
  saveLastModelForProvider,
  saveStoredProviders
} from "./lib/llmProviderStorage";
import {
  KEYBOARD_SHORTCUTS_SETTINGS_TAB,
  saveActiveSettingsTab,
  useAppUiStore,
  canActivateCreatedSession
} from "./lib/appUiStore";
import { formatAskUserAnswerForDisplay } from "./lib/askUser";
import {
  LANGUAGE_CHANGE_EVENT,
  applyHtmlLang,
  applyStoredAppearanceSettings,
  applyTranslucentSidebarSetting,
  languageFromChangeEvent,
  loadAppLanguage
} from "./lib/appearanceSettings";
import { recordFromUnknown } from "./lib/jsonUtils";

function imageToRequestPayload(image: ImageAttachment) {
  return {
    base64: image.base64,
    mediaType: image.mediaType,
    name: image.name
  };
}

function refreshProviderCache() {
  if (!("__TAURI_INTERNALS__" in window)) {
    return;
  }
  invoke<unknown[]>("config_get_providers")
    .then((providers) => {
      if (Array.isArray(providers)) {
        saveStoredProviders(providers);
      }
    })
    .catch(console.error);
}

function homePathFromWorkspace(workspacePath: string) {
  const macHome = workspacePath.match(/^\/Users\/[^/]+/);
  if (macHome) return macHome[0];
  const linuxHome = workspacePath.match(/^\/home\/[^/]+/);
  if (linuxHome) return linuxHome[0];
  return workspacePath;
}

function parseDurationFromTitle(title?: string) {
  if (!title) return null;
  const minuteSecond = title.match(/(\d+)\s*(?:分|m|min|分钟)\s*(\d+)?\s*(?:秒|s)?/i);
  if (minuteSecond) {
    return Number(minuteSecond[1]) * 60 + Number(minuteSecond[2] || 0);
  }
  const seconds = title.match(/(\d+)\s*(?:秒|s|sec|seconds?)/i);
  if (seconds) return Number(seconds[1]);
  return null;
}

function timelineCreatedAt(item: TimelineItem | null | undefined): number | undefined {
  return typeof item?.createdAt === "number" && Number.isFinite(item.createdAt)
    ? item.createdAt
    : undefined;
}

function turnStaticDurationSeconds(turn: ConversationTurn) {
  for (const item of turn.items) {
    if (item.kind === "reasoning") {
      const parsed = parseDurationFromTitle(item.title);
      if (parsed !== null) return parsed;
    }
  }

  const createdTimes = [turn.userItem, ...turn.items]
    .map(timelineCreatedAt)
    .filter((value): value is number => value !== undefined);
  if (createdTimes.length >= 2) {
    return Math.max(1, Math.round((Math.max(...createdTimes) - Math.min(...createdTimes)) / 1000));
  }
  return 0;
}

function formatDurationZh(totalSeconds: number) {
  const safeSeconds = Math.max(0, Math.round(totalSeconds));
  const minutes = Math.floor(safeSeconds / 60);
  const seconds = safeSeconds % 60;
  if (minutes <= 0) return `${seconds} 秒`;
  if (seconds <= 0) return `${minutes} 分钟`;
  return `${minutes} 分 ${seconds} 秒`;
}

function upsertSessionKeepingActive(
  items: SessionSummary[],
  session: SessionSummary,
  activeSessionId: string | null
) {
  if (activeSessionId === session.id) {
    return upsertActiveSession(items, session);
  }
  const nextSession = { ...session, active: false };
  return items.some((item) => item.id === session.id)
    ? items.map((item) => (item.id === session.id ? nextSession : item))
    : [...items, nextSession];
}

export function App() {
  const bootstrap = useAppUiStore((state) => state.bootstrap);
  const setBootstrap = useAppUiStore((state) => state.setBootstrap);
  const viewMode = useAppUiStore((state) => state.viewMode);
  const setViewMode = useAppUiStore((state) => state.setViewMode);
  const appLang = useAppUiStore((state) => state.appLang);
  const setAppLang = useAppUiStore((state) => state.setAppLang);
  const sessionItems = useAppUiStore((state) => state.sessionItems);
  const setSessionItems = useAppUiStore((state) => state.setSessionItems);
  const timelineItems = useAppUiStore((state) => state.timelineItems);
  const setTimelineItems = useAppUiStore((state) => state.setTimelineItems);
  const activeSessionId = useAppUiStore((state) => state.activeSessionId);
  const setActiveSessionId = useAppUiStore((state) => state.setActiveSessionId);
  const draft = useAppUiStore((state) => state.draft);
  const setDraft = useAppUiStore((state) => state.setDraft);
  const projectRoots = useAppUiStore((state) => state.projectRoots);
  const setProjectRoots = useAppUiStore((state) => state.setProjectRoots);
  const projectOrder = useAppUiStore((state) => state.projectOrder);
  const setProjectOrder = useAppUiStore((state) => state.setProjectOrder);
  const selectedProjectRoot = useAppUiStore((state) => state.selectedProjectRoot);
  const setSelectedProjectRoot = useAppUiStore((state) => state.setSelectedProjectRoot);
  const reloadProjectStorage = useAppUiStore((state) => state.reloadProjectStorage);
  const inspectorOpen = useAppUiStore((state) => state.inspectorOpen);
  const setInspectorOpen = useAppUiStore((state) => state.setInspectorOpen);
  const sidebarOpen = useAppUiStore((state) => state.sidebarOpen);
  const setSidebarOpen = useAppUiStore((state) => state.setSidebarOpen);
  const sidebarWidth = useAppUiStore((state) => state.sidebarWidth);
  const setSidebarWidth = useAppUiStore((state) => state.setSidebarWidth);
  const inspectorWidth = useAppUiStore((state) => state.inspectorWidth);
  const setInspectorWidth = useAppUiStore((state) => state.setInspectorWidth);
  const terminalHeight = useAppUiStore((state) => state.terminalHeight);
  const setTerminalHeight = useAppUiStore((state) => state.setTerminalHeight);
  const terminalOpenByConversation = useAppUiStore((state) => state.terminalOpenByConversation);
  const setTerminalOpenForConversation = useAppUiStore((state) => state.setTerminalOpenForConversation);
  const generalSettings = useAppUiStore((state) => state.generalSettings);
  const refreshGeneralSettings = useAppUiStore((state) => state.refreshGeneralSettings);
  const permissionMode = useAppUiStore((state) => state.permissionMode);
  const setPermissionMode = useAppUiStore((state) => state.setPermissionMode);
  const isProcessing = useAppUiStore((state) => state.isProcessing);
  const setIsProcessing = useAppUiStore((state) => state.setIsProcessing);
  const messageQueue = useAppUiStore((state) => state.messageQueue);
  const setMessageQueue = useAppUiStore((state) => state.setMessageQueue);
  const composerImages = useAppUiStore((state) => state.composerImages);
  const setComposerImages = useAppUiStore((state) => state.setComposerImages);
  const currentTurnId = useAppUiStore((state) => state.currentTurnId);
  const setCurrentTurnId = useAppUiStore((state) => state.setCurrentTurnId);
  const pendingUserQuestion = useAppUiStore((state) => state.pendingUserQuestion);
  const setPendingUserQuestion = useAppUiStore((state) => state.setPendingUserQuestion);
  const usageSnapshot = useAppUiStore((state) => state.usageSnapshot);
  const setUsageSnapshot = useAppUiStore((state) => state.setUsageSnapshot);
  const clearTurnState = useAppUiStore((state) => state.clearTurnState);
  const getSessionUiState = useAppUiStore((state) => state.getSessionUiState);
  const promoteDraftToSession = useAppUiStore((state) => state.promoteDraftToSession);
  const activeSessionIdRef = useRef<string | null>(null);
  const draftRequestSequenceRef = useRef(0);
  const windowFocusedRef = useRef(true);
  // 取消轮询登记簿：按 (sessionId, turnId) 幂等维护 watcher 与 pending 登记，
  // 终态事件/新 turn/切会话/卸载时自动清理，杜绝旧 watchdog 解锁新任务。
  const cancellationWatchdogRegistryRef = useRef(new CancellationWatchdogRegistry());
  const terminalConversationKey = activeSessionId ?? "__draft__";
  const terminalOpen = terminalOpenByConversation[terminalConversationKey] ?? false;
  const setTerminalOpenForCurrentConversation = (open: boolean) => {
    setTerminalOpenForConversation(terminalConversationKey, open);
  };
  const draggingPane = useAppUiStore((state) => state.draggingPane);
  const setDraggingPane = useAppUiStore((state) => state.setDraggingPane);
  const dragStateRef = useRef<PaneDragState | null>(null);
  const dragCaptureRef = useRef<{ target: Element; pointerId: number } | null>(null);

  const updateCancellationBoundary = useCallback(
    (sessionId: string, turnId: string, title: string, body: string) => {
      setTimelineItems((items) => {
        const existing = items.some(
          (item) => item.kind === "boundary" && item.id.includes(`cancel-${turnId}-`)
        );
        if (!existing) {
          return [
            ...items,
            {
              id: `cancel-${turnId}-${Date.now()}`,
              kind: "boundary" as const,
              title,
              body,
              createdAt: Date.now()
            }
          ];
        }
        return items.map((item) =>
          item.kind === "boundary" && item.id.includes(`cancel-${turnId}-`)
            ? { ...item, title, body }
            : item
        );
      }, sessionId);
    },
    [setTimelineItems]
  );

  const createCancellationWatchdog = useCallback(
    (sessionId: string, turnId: string, onConfirmedTerminal: () => void) =>
      startCancellationWatchdog({
        sessionId,
        turnId,
        // 会话 + turn 双重隔离：只有该会话的当前 turn 仍是被取消的 turn 才会继续
        isStillCurrent: () => getSessionUiState(sessionId).currentTurnId === turnId,
        fetchRuns: () => invoke<RunState[]>("runs_list"),
        onReleased: () => {
          // 轮询确认终态：先清理 registry 登记，再解锁 UI
          onConfirmedTerminal();
          let released = false;
          setCurrentTurnId((turnIdNow) => {
            if (turnIdNow !== turnId) return turnIdNow;
            released = true;
            return null;
          }, sessionId);
          if (!released) return; // turn 已切换：不得解锁新任务
          discardPendingDeltas(sessionId);
          setIsProcessing(false, sessionId);
          setPendingUserQuestion(
            (question) => (question?.turnId === turnId ? null : question),
            sessionId
          );
          updateCancellationBoundary(sessionId, turnId, "已结束", "后端已确认本轮运行结束。");
        },
        onWaiting: () => {
          updateCancellationBoundary(sessionId, turnId, "仍在停止", "仍在等待后端确认本轮运行已结束。");
        },
        onQueryError: () => {
          updateCancellationBoundary(sessionId, turnId, "仍在停止", "暂时无法确认停止状态，仍在等待后端终态事件。");
        },
        schedule: (callback, delayMs) => window.setTimeout(callback, delayMs),
        clear: (handle) => window.clearTimeout(handle)
      }),
    [getSessionUiState, setCurrentTurnId, setIsProcessing, setPendingUserQuestion, updateCancellationBoundary]
  );

  useEffect(() => {
    activeSessionIdRef.current = activeSessionId;
  }, [activeSessionId]);

  const currentTurnIdRef = useRef<string | null>(null);
  useEffect(() => {
    currentTurnIdRef.current = currentTurnId;
  }, [currentTurnId]);

  // 切换会话：暂停被离开会话的取消轮询定时器（保留 pending 登记）；
  // 切回时若该会话仍处于“已取消未终态”状态，则重新武装对账轮询
  // （双重隔离由 watchdog 守卫，旧 watchdog 绝不解锁新任务）。
  const previousActiveSessionIdRef = useRef<string | null>(null);
  useEffect(() => {
    const previous = previousActiveSessionIdRef.current;
    previousActiveSessionIdRef.current = activeSessionId;
    if (previous !== null && previous !== activeSessionId) {
      cancellationWatchdogRegistryRef.current.suspendSession(previous);
    }
  }, [activeSessionId]);

  useEffect(() => {
    if (!activeSessionId) return;
    const ui = getSessionUiState(activeSessionId);
    if (!ui.isProcessing || !ui.currentTurnId) return;
    cancellationWatchdogRegistryRef.current
      .resume(activeSessionId, ui.currentTurnId, createCancellationWatchdog)
      ?.start();
  }, [activeSessionId, getSessionUiState, createCancellationWatchdog]);

  useEffect(() => {
    const handleFocus = () => {
      windowFocusedRef.current = true;
    };
    const handleBlur = () => {
      windowFocusedRef.current = false;
    };
    const handleVisibility = () => {
      windowFocusedRef.current = document.visibilityState === "visible";
    };
    window.addEventListener("focus", handleFocus);
    window.addEventListener("blur", handleBlur);
    document.addEventListener("visibilitychange", handleVisibility);
    return () => {
      window.removeEventListener("focus", handleFocus);
      window.removeEventListener("blur", handleBlur);
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, []);

  useEffect(() => {
    const onMove = (event: PointerEvent) => {
      if (!draggingPane || !dragStateRef.current) return;
      const drag = dragStateRef.current;
      const next = computePaneDragSize(
        draggingPane,
        drag,
        { clientX: event.clientX, clientY: event.clientY },
        window.innerHeight
      );

      if (draggingPane === "sidebar") {
        setSidebarWidth(next);
        setSidebarOpen(!isPaneCollapsed("sidebar", next));
      } else if (draggingPane === "inspector") {
        setInspectorWidth(next);
        setInspectorOpen(!isPaneCollapsed("inspector", next));
      } else if (draggingPane === "terminal") {
        setTerminalHeight(next);
        if (isPaneCollapsed("terminal", next)) {
          setTerminalOpenForCurrentConversation(false);
        }
      }
    };

    const releaseDragCapture = () => {
      const capture = dragCaptureRef.current;
      if (capture && "releasePointerCapture" in capture.target) {
        try {
          (capture.target as HTMLElement).releasePointerCapture(capture.pointerId);
        } catch {
          // pointerup/cancel 后浏览器可能已经自动释放。
        }
      }
      dragCaptureRef.current = null;
    };

    const onUp = () => {
      releaseDragCapture();
      const finishedPane = draggingPane;
      // 拖动结束：将最终尺寸一次性写入 localStorage
      if (finishedPane === "sidebar") {
        localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(sidebarWidth));
      } else if (finishedPane === "inspector") {
        localStorage.setItem(INSPECTOR_WIDTH_STORAGE_KEY, String(inspectorWidth));
      } else if (finishedPane === "terminal") {
        localStorage.setItem(TERMINAL_HEIGHT_STORAGE_KEY, String(terminalHeight));
      }
      setDraggingPane(null);
      dragStateRef.current = null;
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    window.addEventListener("blur", onUp);
    return () => {
      releaseDragCapture();
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      window.removeEventListener("blur", onUp);
    };
  }, [draggingPane, sidebarWidth, inspectorWidth, terminalHeight]);

  const beginPaneDrag = (pane: PaneKind, event: React.PointerEvent) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
      dragCaptureRef.current = {
        target: event.currentTarget,
        pointerId: event.pointerId
      };
    } catch {
      dragCaptureRef.current = null;
    }
    dragStateRef.current = {
      startX: event.clientX,
      startY: event.clientY,
      startSidebarWidth: sidebarWidth,
      startInspectorWidth: inspectorWidth,
      startTerminalHeight: terminalHeight
    };
    setDraggingPane(pane);
    if (pane === "terminal") {
      setTerminalOpenForCurrentConversation(true);
    } else if (pane === "inspector") {
      setInspectorOpen(true);
    } else if (pane === "sidebar") {
      setSidebarOpen(true);
    }
  };

  const handlePermissionModeChange = (mode: string) => {
    setPermissionMode(mode);
    setBootstrap(prev => ({ ...prev, permissionMode: mode }));
    invoke("permission_mode_set", { mode }).catch(console.error);
  };

  const handleUpdateProvider = async (provider: string) => {
    const defaultModel = preferredModelFromStorage(provider, PROVIDERS_META);

    if (activeSessionId) {
      setSessionItems((items) =>
        items.map((s) =>
          s.id === activeSessionId ? { ...s, provider, model: defaultModel } : s
        )
      );
      try {
        await invoke("sessions_update_llm", {
          sessionId: activeSessionId,
          provider,
          model: defaultModel
        });
      } catch (err) {
        console.error(err);
      }
    } else {
      setBootstrap((prev) => ({ ...prev, provider, model: defaultModel }));
    }
  };

  const handleUpdateModel = async (model: string) => {
    saveLastModelForProvider(currentProvider, model);

    if (activeSessionId) {
      setSessionItems((items) =>
        items.map((s) =>
          s.id === activeSessionId ? { ...s, model } : s
        )
      );
      try {
        await invoke("sessions_update_llm", {
          sessionId: activeSessionId,
          provider: currentProvider,
          model
        });
      } catch (err) {
        console.error(err);
      }
    } else {
      setBootstrap((prev) => ({ ...prev, model }));
    }
  };

  useEffect(() => {
    const handleLangChange = (e: Event) => {
      setAppLang(languageFromChangeEvent(e));
      applyHtmlLang(languageFromChangeEvent(e));
    };
    window.addEventListener(LANGUAGE_CHANGE_EVENT, handleLangChange);
    return () => window.removeEventListener(LANGUAGE_CHANGE_EVENT, handleLangChange);
  }, []);

  useEffect(() => {
    const handleGeneralSettingsChange = () => {
      refreshGeneralSettings();
    };
    refreshGeneralSettings();
    window.addEventListener(GENERAL_SETTINGS_CHANGE_EVENT, handleGeneralSettingsChange);
    return () => window.removeEventListener(GENERAL_SETTINGS_CHANGE_EVENT, handleGeneralSettingsChange);
  }, [refreshGeneralSettings]);

  const sendSystemNotification = useCallback((title: string, body: string, policy: "completion" | "permission" | "question") => {
    if (!("Notification" in window)) return;
    if (policy === "permission" && !generalSettings.permissionNotification) return;
    if (policy === "question" && !generalSettings.questionNotification) return;
    if (policy === "completion") {
      if (generalSettings.completionNotification === "Never") return;
      if (generalSettings.completionNotification === "Only when unfocused" && windowFocusedRef.current) return;
    }
    const show = () => new Notification(title, { body });
    if (Notification.permission === "granted") {
      show();
    } else if (Notification.permission === "default") {
      Notification.requestPermission().then((permission) => {
        if (permission === "granted") show();
      }).catch(console.error);
    }
  }, [generalSettings]);

  useEffect(() => {
    const handleProjectRootsChanged = () => {
      reloadProjectStorage();
    };
    window.addEventListener(PROJECT_ROOTS_CHANGED_EVENT, handleProjectRootsChanged);
    window.addEventListener("storage", handleProjectRootsChanged);
    return () => {
      window.removeEventListener(PROJECT_ROOTS_CHANGED_EVENT, handleProjectRootsChanged);
      window.removeEventListener("storage", handleProjectRootsChanged);
    };
  }, [reloadProjectStorage]);

  useEffect(() => {
    applyStoredAppearanceSettings();
    applyHtmlLang(loadAppLanguage());
  }, []);

  useEffect(() => {
    applyTranslucentSidebarSetting();
  }, [viewMode]);

  const loadBootstrap = () => {
    refreshProviderCache();
    invoke<Bootstrap>("app_get_bootstrap")
      .then((nextBootstrap) => {
        setBootstrap(nextBootstrap);
        setPermissionMode(nextBootstrap.permissionMode);
        setSelectedProjectRoot((current) =>
          current === undefined || current === fallbackBootstrap.workspacePath
            ? nextBootstrap.workspacePath
            : current
        );

        const activeSessions = visibleSessions(nextBootstrap.sessions);
        const activeSessionId = activeSessions.find((session) => session.active)?.id ?? null;

        setSessionItems(activeSessions);
        setProjectRoots((current) =>
          dedupeProjectRoots([
            ...current,
            ...activeSessions.map((session) => session.projectRoot),
          ])
        );
        activeSessionIdRef.current = activeSessionId;
        setActiveSessionId(activeSessionId);
        if (activeSessionId && "__TAURI_INTERNALS__" in window) {
          invoke<DesktopMessage[]>("sessions_messages", {
            sessionId: activeSessionId,
            session_id: activeSessionId
          })
            .then((messages) => {
              const sessionUiState = getSessionUiState(activeSessionId);
              if (
                activeSessionIdRef.current !== activeSessionId ||
                sessionUiState.timelineItems.length > 0 ||
                sessionUiState.isProcessing ||
                sessionUiState.pendingUserQuestion
              ) return;
              setTimelineItems(messagesToTimelineItems(messages));
            })
            .catch(console.error);
        }
      })
      .catch(() => {
        setBootstrap(fallbackBootstrap);
        if (!("__TAURI_INTERNALS__" in window)) {
          setSessionItems([]);
          activeSessionIdRef.current = null;
          setActiveSessionId(null);
          setSelectedProjectRoot((current) =>
            current === undefined ? fallbackBootstrap.workspacePath : current
          );
          setTimelineItems([]);
        }
      });
  };

  useEffect(() => {
    loadBootstrap();
  }, []);

  useEffect(() => {
    const handleUnarchive = () => {
      loadBootstrap();
    };
    const handleDefaultLlmChange = (event: Event) => {
      const detail = detailFromDefaultLlmChangeEvent(event);
      if (!detail) {
        loadBootstrap();
        return;
      }
      setBootstrap((current) => ({
        ...current,
        provider: detail.provider,
        model: detail.model
      }));
    };
    const handlePermanentDelete = (event: Event) => {
      const detail = detailFromSessionIdEvent(event);
      if (!detail) {
        loadBootstrap();
        return;
      }
      setSessionItems((items) => items.filter((session) => session.id !== detail.sessionId));
      if (activeSessionIdRef.current === detail.sessionId) {
        activeSessionIdRef.current = null;
        setActiveSessionId(null);
      }
    };
    window.addEventListener(SESSION_UNARCHIVED_EVENT, handleUnarchive);
    window.addEventListener(DEFAULT_LLM_CHANGE_EVENT, handleDefaultLlmChange);
    window.addEventListener(SESSION_DELETED_PERMANENTLY_EVENT, handlePermanentDelete);
    window.addEventListener(SESSIONS_IMPORTED_EVENT, handleUnarchive);
    return () => {
      window.removeEventListener(SESSION_UNARCHIVED_EVENT, handleUnarchive);
      window.removeEventListener(DEFAULT_LLM_CHANGE_EVENT, handleDefaultLlmChange);
      window.removeEventListener(SESSION_DELETED_PERMANENTLY_EVENT, handlePermanentDelete);
      window.removeEventListener(SESSIONS_IMPORTED_EVENT, handleUnarchive);
    };
  }, []);

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) {
      return;
    }

    let active = true;
    let disposeFn: (() => void) | undefined;

    listen<DesktopEvent>("desktop-event", (event) => {
      if (!active) return;
      handleDesktopRuntimeEvent({
        activeSessionId: activeSessionIdRef.current,
        currentTurnId: currentTurnIdRef.current,
        getCurrentTurnId: (sessionId) => getSessionUiState(sessionId).currentTurnId,
        payload: event.payload,
        sendSystemNotification,
        setCurrentTurnId,
        setIsProcessing,
        setPendingUserQuestion,
        setTimelineItems,
        setUsageSnapshot
      });
      // 同一 turn 的后端终态事件到达：立即停止取消轮询并清理登记，
      // 避免重复解锁或空转。
      const terminal = isTerminalTurnEvent(event.payload);
      if (terminal) {
        cancellationWatchdogRegistryRef.current.stop(terminal.sessionId, terminal.turnId);
      }
    })
      .then((dispose) => {
        if (!active) {
          dispose();
        } else {
          disposeFn = dispose;
        }
      })
      .catch(console.error);

    return () => {
      active = false;
      if (disposeFn) {
        disposeFn();
      }
      // 组件卸载：清理全部取消轮询定时器与登记
      cancellationWatchdogRegistryRef.current.stopAll();
    };
  }, [getSessionUiState, sendSystemNotification, createCancellationWatchdog]);

  const activeSession = useMemo(
    () =>
      activeSessionId
        ? sessionItems.find((session) => session.id === activeSessionId) ?? null
        : null,
    [activeSessionId, sessionItems]
  );
  const currentProvider = activeSession?.provider ?? bootstrap.provider;
  const currentModel = activeSession?.model ?? bootstrap.model;

  const projectOptions = useMemo(() => {
    const roots = dedupeProjectRoots([
      bootstrap.workspacePath,
      ...projectRoots,
      ...sessionItems.map((session) => session.projectRoot),
    ]);
    return [
      ...roots.map((root) => ({
        label: projectLabelFromPath(root),
        root,
      })),
      { label: "独立对话", root: null }
    ];
  }, [bootstrap.workspacePath, projectRoots, sessionItems]);

  useEffect(() => {
    const roots = projectOptions
      .map((option) => option.root)
      .filter((root): root is string => Boolean(root));
    setProjectOrder((current) => [
      ...current.filter((root) => roots.includes(root)),
      ...roots.filter((root) => !current.includes(root)),
    ]);
  }, [projectOptions]);

  const orderedProjectOptions = useMemo(() => {
    const orderIndex = new Map(projectOrder.map((root, index) => [root, index]));
    return [...projectOptions].sort((a, b) => {
      if (!a.root || !b.root) {
        return a.root ? -1 : b.root ? 1 : 0;
      }
      return (orderIndex.get(a.root) ?? Number.MAX_SAFE_INTEGER) -
        (orderIndex.get(b.root) ?? Number.MAX_SAFE_INTEGER);
    });
  }, [projectOptions, projectOrder]);

  const handleProjectReorder = (draggedRoot: string, targetRoot: string, placement: "before" | "after" = "before") => {
    if (draggedRoot === targetRoot) return;
    setProjectOrder((current) => {
      const roots = projectOptions
        .map((option) => option.root)
        .filter((root): root is string => Boolean(root));
      const base = [
        ...current.filter((root) => roots.includes(root)),
        ...roots.filter((root) => !current.includes(root)),
      ];
      const from = base.indexOf(draggedRoot);
      const targetIndex = base.indexOf(targetRoot);
      if (from < 0 || targetIndex < 0) return base;
      const withoutDragged = base.filter((root) => root !== draggedRoot);
      const to = withoutDragged.indexOf(targetRoot);
      if (from < 0 || to < 0) return base;
      const insertIndex = placement === "after" ? to + 1 : to;
      const next = [...withoutDragged];
      next.splice(insertIndex, 0, draggedRoot);
      return next;
    });
  };

  function handleCreateSession(projectRoot?: string | null) {
    const recentSession = sessionItems.find((session) => session.provider && session.model);
    if (recentSession?.provider && recentSession.model) {
      setBootstrap((current) => ({
        ...current,
        provider: recentSession.provider!,
        model: recentSession.model!
      }));
    }
    // 切到独立草稿槽；旧会话可能仍在后台运行，不能清空其事件轨道或运行态。
    draftRequestSequenceRef.current += 1;
    activeSessionIdRef.current = null;
    setActiveSessionId(null);
    setSessionItems((items) => items.map((item) => ({ ...item, active: false })));
    setTimelineItems([]);
    if (projectRoot !== undefined) {
      setSelectedProjectRoot(projectRoot);
    }
  }

  async function handleAddProject() {
    const pickedRoot = await invoke<string | null>("project_folder_pick").catch((err) => {
      console.error(err);
      return null;
    });
    const normalized = normalizeProjectRoot(pickedRoot);
    if (!normalized) return;
    setProjectRoots((current) => dedupeProjectRoots([...current, normalized]));
    setSelectedProjectRoot(normalized);
  }

  async function handleAskUserResolve(answer: string): Promise<boolean> {
    if (!pendingUserQuestion) return false;
    const displayText = formatAskUserAnswerForDisplay(answer);
    const question = pendingUserQuestion;

    try {
      await invoke("ask_user_respond", {
        sessionId: question.sessionId,
        session_id: question.sessionId,
        turnId: question.turnId,
        turn_id: question.turnId,
        answer: answer
      });
      setTimelineItems(
        (items) => [
          ...items,
          {
            id: `ask-answer-${Date.now()}`,
            kind: "user",
            title: "用户",
            body: displayText,
            createdAt: Date.now()
          }
        ],
        question.sessionId
      );
      setPendingUserQuestion(
        (current) => (current?.turnId === question.turnId ? null : current),
        question.sessionId
      );
      return true;
    } catch (err) {
      console.error(err);
      setTimelineItems(
        (items) => [
          ...items,
          {
            id: `ask-answer-error-${Date.now()}`,
            kind: "assistant",
            title: "错误",
            body: "发送问题回复失败，请重试。",
            meta: "stream complete"
          }
        ],
        question.sessionId
      );
      return false;
    }
  }

  function appendLocalCommandResult(title: string, body: string) {
    setTimelineItems((items) => [
      ...items,
      {
        id: `local-command-${Date.now()}-${Math.random().toString(16).slice(2)}`,
        kind: "assistant",
        title,
        body,
        meta: "local command",
        createdAt: Date.now()
      }
    ]);
  }

  async function handleLocalSlashCommand(content: string) {
    return executeLocalSlashCommand(content, {
      activeSession,
      activeSessionId,
      appLang,
      bootstrapWorkspacePath: bootstrap.workspacePath,
      currentModel,
      currentProvider,
      isProcessing,
      permissionMode,
      selectedProjectRoot,
      sessionItems,
      timelineItemCount: timelineItems.length,
      usageSnapshot,
      appendResult: appendLocalCommandResult,
      createSession: handleCreateSession,
      clearMessageQueue: () => setMessageQueue([]),
      setPendingUserQuestion,
      setPermissionMode,
      setSessionItems,
      setTimelineItems,
      setUsageSnapshot
    });
  }

  async function handleSendMessage() {
    if (!draft.trim() && composerImages.length === 0) return;
    const content = draft.trim();
    const imagesAtSend = composerImages;

    if (pendingUserQuestion) {
      const didResolve = await handleAskUserResolve(content);
      if (didResolve) {
        setDraft("");
        setComposerImages([]);
      }
      return;
    }

    if (imagesAtSend.length === 0 && await handleLocalSlashCommand(content)) {
      setDraft("");
      setComposerImages([]);
      return;
    }

    const isDetachedReview = generalSettings.codeReviewPolicy === "detached" && /^\/review\b/i.test(content);
    const sessionIdAtSend = isDetachedReview ? null : (activeSession?.id ?? null);
    const projectRootAtSend = selectedProjectRoot === undefined ? bootstrap.workspacePath : selectedProjectRoot;
    const sourceSessionIdAtSend = activeSessionIdRef.current;
    if (isDetachedReview) {
      // 独立审查不能借用当前会话的 UI 状态，否则发送失败或事件迟到时会污染原会话。
      setDraft("", sourceSessionIdAtSend);
      setComposerImages([], sourceSessionIdAtSend);
      activeSessionIdRef.current = null;
      setActiveSessionId(null);
      setDraft("");
      setComposerImages([]);
    } else {
      setDraft("", sourceSessionIdAtSend);
      setComposerImages([], sourceSessionIdAtSend);
    }
    const stateSessionIdAtSend = activeSessionIdRef.current;
    const draftRequestSequence = stateSessionIdAtSend === null
      ? ++draftRequestSequenceRef.current
      : null;

    if (isProcessing && !isDetachedReview) {
      setMessageQueue((prev) => [...prev, { content, images: imagesAtSend }]);
      setTimelineItems((items) => [
        ...items,
        {
          id: `local-queued-${Date.now()}`,
          kind: "user",
          title: "用户 (等待中...)",
          body: content,
          attachments: imagesAtSend,
          createdAt: Date.now()
        }
      ]);
      return;
    }

    setIsProcessing(true, stateSessionIdAtSend);
    // 新 turn 开始前重置合帧状态：上一 turn 的残留增量/轨道指向
    // 不得影响新 turn（含 detached review 创建的全新会话）
    if (stateSessionIdAtSend) discardPendingDeltas(stateSessionIdAtSend);
    setTimelineItems(
      (items) => [
        ...items,
        {
          id: `local-${Date.now()}`,
          kind: "user",
          title: "用户",
          body: content,
          attachments: imagesAtSend,
          createdAt: Date.now()
        }
      ],
      stateSessionIdAtSend
    );

    try {
      const res = await invoke<TurnAccepted>("turn_send_message", {
        request: {
          sessionId: sessionIdAtSend,
          content,
          images: imagesAtSend.map(imageToRequestPayload),
          projectRoot: sessionIdAtSend ? undefined : projectRootAtSend,
          standalone: sessionIdAtSend ? undefined : projectRootAtSend === null,
          title: sessionIdAtSend
            ? undefined
            : isDetachedReview
              ? "代码审查"
              : content
              ? deriveSessionTitle(content)
              : imagesAtSend.length > 1
                ? `${imagesAtSend.length} 张图片`
                : "图片",
          provider: currentProvider,
          model: currentModel
        }
      });
      if (stateSessionIdAtSend === null) {
        promoteDraftToSession(res.sessionId);
        if (canActivateCreatedSession(
          activeSessionIdRef.current,
          draftRequestSequence,
          draftRequestSequenceRef.current
        )) {
          activeSessionIdRef.current = res.sessionId;
          setActiveSessionId(res.sessionId);
        }
      }
      // 新 turn 接管该会话：旧的取消轮询（若仍存在）立即失效并清理
      cancellationWatchdogRegistryRef.current.stopSession(res.sessionId);
      setCurrentTurnId(res.turnId, res.sessionId);
      setIsProcessing(true, res.sessionId);
      setSessionItems((items) =>
        upsertSessionKeepingActive(items, res.session, activeSessionIdRef.current)
      );
    } catch (err) {
      console.error(err);
      setIsProcessing(false, stateSessionIdAtSend);
      setDraft(content, stateSessionIdAtSend);
      setComposerImages(imagesAtSend, stateSessionIdAtSend);
      setTimelineItems(
        (items) => [
          ...items,
          {
            id: `send-error-${Date.now()}`,
            kind: "assistant",
            title: "发送失败",
            body: String(err || "发送失败，请重试。"),
            meta: "stream complete"
          }
        ],
        stateSessionIdAtSend
      );
    }
  }

  useEffect(() => {
    if (!isProcessing && messageQueue.length > 0 && activeSession?.id) {
      const queuedSessionId = activeSession.id;
      const nextMessage = messageQueue[0];
      const nextContent = nextMessage.content;
      setMessageQueue((prev) => prev.slice(1));
      setIsProcessing(true, queuedSessionId);
      discardPendingDeltas(queuedSessionId);
      
      setTimelineItems(
        (items) =>
          items.map((item) =>
            item.kind === "user" && item.body === nextContent && item.title.includes("等待中")
              ? { ...item, title: "用户" }
              : item
          ),
        queuedSessionId
      );

      invoke<TurnAccepted>("turn_send_message", {
        request: {
          sessionId: queuedSessionId,
          content: nextContent,
          images: nextMessage.images.map(imageToRequestPayload),
          projectRoot: undefined,
          standalone: undefined,
          title: undefined,
          provider: undefined,
          model: undefined
        }
      }).then((res) => {
        // 新 turn 接管该会话：旧的取消轮询（若仍存在）立即失效并清理
        cancellationWatchdogRegistryRef.current.stopSession(res.sessionId);
        setCurrentTurnId(res.turnId, res.sessionId);
        setIsProcessing(true, res.sessionId);
        setSessionItems((items) =>
          upsertSessionKeepingActive(items, res.session, activeSessionIdRef.current)
        );
      }).catch((err) => {
        // 发送失败（如 in-flight 拒绝）：队列项在发送前已出队，不会无限重试
        console.error(err);
        setIsProcessing(false, queuedSessionId);
        setTimelineItems(
          (items) => [
            ...items,
            {
              id: `queue-send-error-${Date.now()}`,
              kind: "assistant",
              title: "发送失败",
              body: String(err || "发送失败，请重试。"),
              meta: "stream complete"
            }
          ],
          queuedSessionId
        );
      });
    }
  }, [isProcessing, messageQueue, activeSession?.id]);

  async function handleCancelMessage() {
    if (!(activeSession?.id && currentTurnId)) return;
    const cancelledSessionId = activeSession.id;
    const cancelledTurnId = currentTurnId;
    const isCancelledTurnStillCurrent = () =>
      getSessionUiState(cancelledSessionId).currentTurnId === cancelledTurnId;
    const startWatchdog = () => {
      const registry = cancellationWatchdogRegistryRef.current;
      registry.markPending(cancelledSessionId, cancelledTurnId);
      registry.arm(cancelledSessionId, cancelledTurnId, createCancellationWatchdog).start();
    };

    try {
      await invoke("turn_cancel", {
        sessionId: cancelledSessionId,
        turnId: cancelledTurnId
      });
    } catch (err) {
      console.error(err);
      if (!isCancelledTurnStillCurrent()) return;
      updateCancellationBoundary(cancelledSessionId, cancelledTurnId, "取消请求未确认", "取消请求发送失败，仍在等待后端终态确认。");
      startWatchdog();
      return;
    }

    if (!isCancelledTurnStillCurrent()) return;
    updateCancellationBoundary(cancelledSessionId, cancelledTurnId, "正在停止", "正在停止本轮运行…");
    startWatchdog();
  }

  async function handleSelectSession(sessionId: string) {
    const nextSession = sessionItems.find((item) => item.id === sessionId);
    const cachedSessionUiState = getSessionUiState(sessionId);
    activeSessionIdRef.current = sessionId;
    setActiveSessionId(sessionId);
    setSelectedProjectRoot(nextSession?.projectRoot ?? null);

    // 已接收后台事件或已有本地历史时，内存快照就是最新真相；不能被历史查询覆盖。
    if (
      cachedSessionUiState.timelineItems.length > 0 ||
      cachedSessionUiState.isProcessing ||
      cachedSessionUiState.pendingUserQuestion
    ) return;

    if (!("__TAURI_INTERNALS__" in window)) return;

    try {
      const messages = await invoke<DesktopMessage[]>("sessions_messages", {
        sessionId,
        session_id: sessionId
      });
      const sessionUiState = getSessionUiState(sessionId);
      if (
        activeSessionIdRef.current !== sessionId ||
        sessionUiState.timelineItems.length > 0 ||
        sessionUiState.isProcessing ||
        sessionUiState.pendingUserQuestion
      ) return;
      setTimelineItems(messagesToTimelineItems(messages));
    } catch (err) {
      if (activeSessionIdRef.current !== sessionId) return;
      console.error(err);
      setTimelineItems([
        {
          id: `history-error-${Date.now()}`,
          kind: "assistant",
          title: "错误",
          body: "加载历史对话失败。",
          meta: "stream complete"
        }
      ]);
    }
  }

  const handleSetViewMode = (mode: ViewMode) => {
    setViewMode(mode);
  };

  const handleDeleteSession = (sessionId: string) => {
    const session = sessionItems.find(s => s.id === sessionId);
    if (!session) return;

    archiveSessionLocally(session);
    setSessionItems(prev => prev.filter(s => s.id !== sessionId));
    cancellationWatchdogRegistryRef.current.stopSession(sessionId);
    if (activeSessionIdRef.current === sessionId) {
      const nextProjectRoot = session.projectRoot ?? null;
      discardPendingDeltas();
      resetTurnTracksForSession(sessionId);
      activeSessionIdRef.current = null;
      setActiveSessionId(null);
      clearTurnState();
      setTimelineItems([]);
      setSelectedProjectRoot(nextProjectRoot);
    }
  };

  const isStandalone = activeSession
    ? !activeSession.projectRoot
    : selectedProjectRoot === null;

  const displayedWorkspacePath = isStandalone
    ? null
    : (activeSession?.projectRoot ?? selectedProjectRoot ?? bootstrap.workspacePath);
  const terminalWorkspacePath = isStandalone
    ? homePathFromWorkspace(bootstrap.workspacePath)
    : (displayedWorkspacePath ?? bootstrap.workspacePath);

  useEffect(() => {
    let shortcutBindings = loadShortcutBindings();
    const refreshBindings = () => {
      shortcutBindings = loadShortcutBindings();
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const editableTarget = target?.closest("input, textarea, select, [contenteditable='true']");
      if (editableTarget && !(event.metaKey || event.ctrlKey || event.altKey)) return;

      const action = findShortcutAction(event, shortcutBindings);
      if (!action) return;

      const jumpMatch = action.match(/^go_to_chat_(\d)$/);
      if (jumpMatch) {
        const nextSession = sessionItems[Number(jumpMatch[1]) - 1];
        if (nextSession) {
          event.preventDefault();
          void handleSelectSession(nextSession.id);
        }
        return;
      }

      switch (action) {
        case "newchat":
        case "quickchat":
          event.preventDefault();
          handleCreateSession(selectedProjectRoot);
          break;
        case "open_folder":
          event.preventDefault();
          void handleAddProject();
          break;
        case "settings":
        case "show_kbd_shortcuts":
          event.preventDefault();
          handleSetViewMode("settings");
          saveActiveSettingsTab(KEYBOARD_SHORTCUTS_SETTINGS_TAB);
          break;
        case "toggle_sidebar":
          event.preventDefault();
          setSidebarOpen(!sidebarOpen);
          break;
        case "toggle_side_panel":
          event.preventDefault();
          setInspectorOpen(!inspectorOpen);
          break;
        case "open_terminal":
          event.preventDefault();
          setTerminalOpenForCurrentConversation(!terminalOpen);
          break;
        case "toggle_bottom_panel": {
          event.preventDefault();
          toggleBottomPanelSetting();
          break;
        }
        case "archive":
          if (activeSessionIdRef.current) {
            event.preventDefault();
            handleDeleteSession(activeSessionIdRef.current);
          }
          break;
        case "copy_session_id":
          if (activeSessionIdRef.current) {
            event.preventDefault();
            void navigator.clipboard?.writeText(activeSessionIdRef.current);
          }
          break;
        case "copy_work_dir":
          if (displayedWorkspacePath) {
            event.preventDefault();
            void navigator.clipboard?.writeText(displayedWorkspacePath);
          }
          break;
        case "close_tab":
          if (viewMode === "settings") {
            event.preventDefault();
            handleSetViewMode("chat");
          }
          break;
        default:
          break;
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener(KEYBOARD_SHORTCUTS_CHANGE_EVENT, refreshBindings);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener(KEYBOARD_SHORTCUTS_CHANGE_EVENT, refreshBindings);
    };
  }, [sessionItems, selectedProjectRoot, terminalOpen, displayedWorkspacePath, viewMode]);

  if (viewMode === "settings") {
    return (
      <main className="app-shell" style={{ display: "block", width: "100vw", height: "100vh", overflow: "hidden" }}>
        <SettingsShell bootstrap={bootstrap} onClose={() => handleSetViewMode("chat")} />
      </main>
    );
  }

  return (
    <main
      className={`app-shell ${sidebarOpen ? "" : "sidebar-collapsed"} ${draggingPane ? "pane-dragging" : ""}`}
      style={{
        "--sidebar-width": `${sidebarWidth}px`,
        "--inspector-width": `${inspectorWidth}px`,
        "--terminal-height": `${terminalHeight}px`
      } as React.CSSProperties}
    >
      <Sidebar
        sessions={sessionItems}
        projectOptions={orderedProjectOptions}
        activeSessionId={activeSessionId}
        viewMode={viewMode}
        onChangeView={handleSetViewMode}
        onCreateSession={handleCreateSession}
        onSelectSession={(sessionId) => {
          void handleSelectSession(sessionId);
        }}
        onAddProject={handleAddProject}
        onProjectReorder={handleProjectReorder}
        onDeleteSession={handleDeleteSession}
      />
      <div
        className="pane-resizer sidebar-resizer"
        onPointerDown={(event) => beginPaneDrag("sidebar", event)}
        role="separator"
        aria-orientation="vertical"
        title="拖动调整侧边栏宽度"
      />
      <section className="workspace" style={{ position: "relative", overflow: "hidden" }}>
        <Topbar
          bootstrap={bootstrap}
          sessionTitle={activeSession?.title ?? (appLang === "zh" ? "新对话" : "New chat")}
          workspacePath={displayedWorkspacePath}
          inspectorOpen={inspectorOpen}
          isProcessing={isProcessing && !pendingUserQuestion}
          onToggleInspector={() => setInspectorOpen(!inspectorOpen)}
          terminalOpen={terminalOpen}
          onToggleTerminal={() => setTerminalOpenForCurrentConversation(!terminalOpen)}
          currentProvider={currentProvider}
          currentModel={currentModel}
          onProviderChange={handleUpdateProvider}
          onModelChange={handleUpdateModel}
        />
        <ChatWorkspace
          draft={draft}
          timelineItems={timelineItems}
          onDraftChange={setDraft}
          images={composerImages}
          onImagesChange={setComposerImages}
          onSendMessage={handleSendMessage}
          inspectorOpen={inspectorOpen}
          inspectorWidth={inspectorWidth}
          onInspectorResizeStart={(event) => beginPaneDrag("inspector", event)}
          isProcessing={isProcessing}
          onCancelMessage={handleCancelMessage}
          permissionMode={permissionMode}
          onPermissionModeChange={handlePermissionModeChange}
          onPermissionResolved={(id) => {
            setTimelineItems((items) => items.filter((item) => item.id !== id));
          }}
          appLang={appLang}
          projectOptions={orderedProjectOptions}
          selectedProjectRoot={selectedProjectRoot === undefined ? bootstrap.workspacePath : selectedProjectRoot}
          onProjectRootChange={setSelectedProjectRoot}
          onAddProject={handleAddProject}
          currentProvider={currentProvider}
          currentModel={currentModel}
          onModelChange={handleUpdateModel}
          pendingUserQuestion={pendingUserQuestion}
          onAskUserResolve={handleAskUserResolve}
          showSuggestedPrompts={generalSettings.suggestedPrompts}
          showBottomPanel={generalSettings.bottomPanel}
          showContextUsage={generalSettings.contextUsage}
          requireOptEnter={generalSettings.requireOptEnter}
          usageSnapshot={usageSnapshot}
        />
        <TerminalDrawer
          isOpen={terminalOpen}
          onClose={() => setTerminalOpenForCurrentConversation(false)}
          workspacePath={terminalWorkspacePath}
          conversationId={activeSessionId}
          height={terminalHeight}
          onResizeStart={(event) => beginPaneDrag("terminal", event)}
        />
      </section>
    </main>
  );
}
