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
import hljs from "highlight.js/lib/core";
// 按需注册常用语言（轻量化）
import langBash from "highlight.js/lib/languages/bash";
import langPython from "highlight.js/lib/languages/python";
import langRust from "highlight.js/lib/languages/rust";
import langTypescript from "highlight.js/lib/languages/typescript";
import langJavascript from "highlight.js/lib/languages/javascript";
import langJson from "highlight.js/lib/languages/json";
import langTOML from "highlight.js/lib/languages/ini";
import langYaml from "highlight.js/lib/languages/yaml";
import langCSS from "highlight.js/lib/languages/css";
import langHTML from "highlight.js/lib/languages/xml";
import langSQL from "highlight.js/lib/languages/sql";
import langC from "highlight.js/lib/languages/c";
import langCpp from "highlight.js/lib/languages/cpp";
import langGo from "highlight.js/lib/languages/go";
import langJava from "highlight.js/lib/languages/java";
import langMarkdown from "highlight.js/lib/languages/markdown";
import langDiff from "highlight.js/lib/languages/diff";
hljs.registerLanguage("bash", langBash);
hljs.registerLanguage("sh", langBash);
hljs.registerLanguage("shell", langBash);
hljs.registerLanguage("zsh", langBash);
hljs.registerLanguage("python", langPython);
hljs.registerLanguage("py", langPython);
hljs.registerLanguage("rust", langRust);
hljs.registerLanguage("rs", langRust);
hljs.registerLanguage("typescript", langTypescript);
hljs.registerLanguage("ts", langTypescript);
hljs.registerLanguage("tsx", langTypescript);
hljs.registerLanguage("javascript", langJavascript);
hljs.registerLanguage("js", langJavascript);
hljs.registerLanguage("jsx", langJavascript);
hljs.registerLanguage("json", langJson);
hljs.registerLanguage("toml", langTOML);
hljs.registerLanguage("ini", langTOML);
hljs.registerLanguage("yaml", langYaml);
hljs.registerLanguage("yml", langYaml);
hljs.registerLanguage("css", langCSS);
hljs.registerLanguage("html", langHTML);
hljs.registerLanguage("xml", langHTML);
hljs.registerLanguage("sql", langSQL);
hljs.registerLanguage("c", langC);
hljs.registerLanguage("cpp", langCpp);
hljs.registerLanguage("go", langGo);
hljs.registerLanguage("java", langJava);
hljs.registerLanguage("md", langMarkdown);
hljs.registerLanguage("markdown", langMarkdown);
hljs.registerLanguage("diff", langDiff);

import {
  Bootstrap,
  DesktopEvent,
  DesktopMessage,
  fallbackBootstrap,
  SessionSummary,
  sessions,
  TimelineItem,
  timeline,
  TurnAccepted,
  ImageAttachment
} from "./lib/mock";
import { SettingsShell } from "./components/SettingsShell";
import { TerminalDrawer } from "./components/TerminalDrawer";
import { PROVIDERS_META } from "./components/settings/ProvidersSettings";
import { Sidebar, ViewMode } from "./components/Sidebar";
import { Topbar } from "./components/Topbar";
import { ChatWorkspace, PendingUserQuestion } from "./components/ChatWorkspace";
import {
  applyDesktopEventToTimelineItems,
  messagesToTimelineItems,
  upsertActiveSession,
  deriveSessionTitle,
  projectLabelFromPath,
  ConversationTurn
} from "./components/timelineUtils";
import { findShortcutAction, loadShortcutBindings } from "./lib/keyboardShortcuts";

const PROJECT_ROOTS_STORAGE_KEY = "yode-project-roots";
const PROJECT_ORDER_STORAGE_KEY = "yode-project-order";
const SELECTED_PROJECT_ROOT_STORAGE_KEY = "yode-selected-project-root";
const ARCHIVED_SESSION_IDS_STORAGE_KEY = "yode-archived-session-ids";
const DELETED_SESSION_IDS_STORAGE_KEY = "yode-deleted-session-ids";
const STANDALONE_PROJECT_SENTINEL = "__standalone__";
const SIDEBAR_WIDTH_STORAGE_KEY = "yode-sidebar-width";
const INSPECTOR_WIDTH_STORAGE_KEY = "yode-inspector-width";
const TERMINAL_HEIGHT_STORAGE_KEY = "yode-terminal-height";

function clampNumber(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function loadStoredNumber(key: string, fallback: number) {
  const raw = localStorage.getItem(key);
  if (!raw) return fallback;
  const parsed = Number(raw);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function loadGeneralSettings() {
  return {
    bottomPanel: localStorage.getItem("yode-bottom-panel") !== "false",
    suggestedPrompts: localStorage.getItem("yode-suggested-prompts") !== "false",
    contextUsage: localStorage.getItem("yode-context-usage") === "true",
    requireOptEnter: localStorage.getItem("yode-require-opt-enter") === "true",
    followUpBehavior: localStorage.getItem("yode-follow-up-behavior") || "queue",
    codeReviewPolicy: localStorage.getItem("yode-code-review-policy") || "inline",
    completionNotification: localStorage.getItem("yode-completion-notif") || "Only when unfocused",
    permissionNotification: localStorage.getItem("yode-perm-notif") !== "false",
    questionNotification: localStorage.getItem("yode-question-notif") !== "false"
  };
}

function loadGeneralSettingsPayload() {
  return {
    workMode: localStorage.getItem("yode-work-mode") || "coding",
    defaultFilePermission: localStorage.getItem("yode-def-perm") !== "false",
    autoReview: localStorage.getItem("yode-auto-review") !== "false",
    fullAccess: localStorage.getItem("yode-full-access") !== "false",
    openDestination: localStorage.getItem("yode-open-dest") || "VS Code",
    showInMenuBar: localStorage.getItem("yode-show-menu-bar") !== "false",
    bottomPanel: localStorage.getItem("yode-bottom-panel") !== "false",
    terminalLocation: localStorage.getItem("yode-term-loc") || "bottom",
    preventSleep: localStorage.getItem("yode-prevent-sleep") === "true",
    codeReviewPolicy: localStorage.getItem("yode-code-review-policy") || "inline",
    suggestedPrompts: localStorage.getItem("yode-suggested-prompts") !== "false",
    contextUsage: localStorage.getItem("yode-context-usage") === "true",
    followUpBehavior: localStorage.getItem("yode-follow-up-behavior") || "queue",
    requireOptEnter: localStorage.getItem("yode-require-opt-enter") === "true",
    completionNotification: localStorage.getItem("yode-completion-notif") || "Only when unfocused",
    permissionNotification: localStorage.getItem("yode-perm-notif") !== "false",
    questionNotification: localStorage.getItem("yode-question-notif") !== "false"
  };
}

function loadStoredProjectRoots(): string[] {
  try {
    const raw = localStorage.getItem(PROJECT_ROOTS_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return dedupeProjectRoots(parsed.filter((value): value is string => typeof value === "string"));
  } catch {
    return [];
  }
}

function loadStoredProjectOrder(): string[] {
  try {
    const raw = localStorage.getItem(PROJECT_ORDER_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return dedupeProjectRoots(parsed.filter((value): value is string => typeof value === "string"));
  } catch {
    return [];
  }
}

function loadStoredSelectedProjectRoot(): string | null | undefined {
  const raw = localStorage.getItem(SELECTED_PROJECT_ROOT_STORAGE_KEY);
  if (raw === null) return undefined;
  return raw === STANDALONE_PROJECT_SENTINEL ? null : raw;
}

function imageToRequestPayload(image: ImageAttachment) {
  return {
    base64: image.base64,
    mediaType: image.mediaType,
    name: image.name
  };
}

function normalizeProjectRoot(root: string | null | undefined) {
  const trimmed = root?.trim();
  return trimmed ? trimmed : null;
}

function dedupeProjectRoots(roots: Array<string | null | undefined>) {
  const seen = new Set<string>();
  const unique: string[] = [];
  roots.forEach((root) => {
    const normalized = normalizeProjectRoot(root);
    if (!normalized || seen.has(normalized)) return;
    seen.add(normalized);
    unique.push(normalized);
  });
  return unique;
}

function loadStoredStringArray(key: string): string[] {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((value): value is string => typeof value === "string");
  } catch {
    return [];
  }
}

function visibleSessions(sessions: SessionSummary[]) {
  const hiddenIds = new Set([
    ...loadStoredStringArray(ARCHIVED_SESSION_IDS_STORAGE_KEY),
    ...loadStoredStringArray(DELETED_SESSION_IDS_STORAGE_KEY),
  ]);
  return sessions.filter((session) => !hiddenIds.has(session.id));
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

function turnStaticDurationSeconds(turn: ConversationTurn) {
  for (const item of turn.items) {
    if (item.kind === "reasoning") {
      const parsed = parseDurationFromTitle(item.title);
      if (parsed !== null) return parsed;
    }
  }

  const createdTimes = [turn.userItem, ...turn.items]
    .map((item) => (item as any)?.createdAt)
    .filter((value): value is number => typeof value === "number" && Number.isFinite(value));
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

export function App() {
  const [bootstrap, setBootstrap] = useState<Bootstrap>(fallbackBootstrap);
  const [viewMode, setViewMode] = useState<ViewMode>(() => {
    return (localStorage.getItem("yode-view-mode") as ViewMode) || "chat";
  });
  const [appLang, setAppLang] = useState(() => localStorage.getItem("yode-language") || "zh");
  const [draft, setDraft] = useState("");
  const [sessionItems, setSessionItems] = useState<SessionSummary[]>([]);
  const [timelineItems, setTimelineItems] = useState<TimelineItem[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [projectRoots, setProjectRoots] = useState<string[]>(() => loadStoredProjectRoots());
  const [projectOrder, setProjectOrder] = useState<string[]>(() => loadStoredProjectOrder());
  const [selectedProjectRoot, setSelectedProjectRoot] = useState<string | null | undefined>(() => loadStoredSelectedProjectRoot());
  const [inspectorOpen, setInspectorOpen] = useState(true);
  const [terminalOpenByConversation, setTerminalOpenByConversation] = useState<Record<string, boolean>>({});
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState(() => clampNumber(loadStoredNumber(SIDEBAR_WIDTH_STORAGE_KEY, 232), 180, 420));
  const [inspectorWidth, setInspectorWidth] = useState(() => clampNumber(loadStoredNumber(INSPECTOR_WIDTH_STORAGE_KEY, 260), 220, 460));
  const [terminalHeight, setTerminalHeight] = useState(() => clampNumber(loadStoredNumber(TERMINAL_HEIGHT_STORAGE_KEY, 280), 180, 520));
  const [isProcessing, setIsProcessing] = useState(false);
  const [messageQueue, setMessageQueue] = useState<Array<{ content: string; images: ImageAttachment[] }>>([]);
  const [generalSettings, setGeneralSettings] = useState(loadGeneralSettings);
  const [composerImages, setComposerImages] = useState<ImageAttachment[]>([]);
  const [currentTurnId, setCurrentTurnId] = useState<string | null>(null);
  const [permissionMode, setPermissionMode] = useState<string>("default");
  const [pendingUserQuestion, setPendingUserQuestion] = useState<PendingUserQuestion | null>(null);
  const activeSessionIdRef = useRef<string | null>(null);
  const windowFocusedRef = useRef(true);
  const terminalConversationKey = activeSessionId ?? "__draft__";
  const terminalOpen = terminalOpenByConversation[terminalConversationKey] ?? false;
  const setTerminalOpenForCurrentConversation = (open: boolean) => {
    setTerminalOpenByConversation((current) => ({
      ...current,
      [terminalConversationKey]: open
    }));
  };
  const [draggingPane, setDraggingPane] = useState<null | "sidebar" | "inspector" | "terminal">(null);
  const dragStateRef = useRef<{ startX: number; startY: number; startSidebarWidth: number; startInspectorWidth: number; startTerminalHeight: number } | null>(null);
  const dragCaptureRef = useRef<{ target: Element; pointerId: number } | null>(null);

  useEffect(() => {
    activeSessionIdRef.current = activeSessionId;
  }, [activeSessionId]);

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
    localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(sidebarWidth));
  }, [sidebarWidth]);

  useEffect(() => {
    localStorage.setItem(INSPECTOR_WIDTH_STORAGE_KEY, String(inspectorWidth));
  }, [inspectorWidth]);

  useEffect(() => {
    localStorage.setItem(TERMINAL_HEIGHT_STORAGE_KEY, String(terminalHeight));
  }, [terminalHeight]);

  useEffect(() => {
    const onMove = (event: PointerEvent) => {
      if (!draggingPane || !dragStateRef.current) return;
      const drag = dragStateRef.current;
      const minSidebar = 180;
      const maxSidebar = 420;
      const minInspector = 220;
      const maxInspector = 460;
      const minTerminal = 180;
      const maxTerminal = Math.floor(window.innerHeight * 0.75);

      if (draggingPane === "sidebar") {
        const next = clampNumber(drag.startSidebarWidth + (event.clientX - drag.startX), minSidebar, maxSidebar);
        setSidebarWidth(next);
        setSidebarOpen(next > minSidebar + 8);
      } else if (draggingPane === "inspector") {
        const next = clampNumber(drag.startInspectorWidth - (event.clientX - drag.startX), minInspector, maxInspector);
        setInspectorWidth(next);
        setInspectorOpen(next > minInspector + 8);
      } else if (draggingPane === "terminal") {
        const next = clampNumber(drag.startTerminalHeight - (event.clientY - drag.startY), minTerminal, maxTerminal);
        setTerminalHeight(next);
        if (next <= minTerminal + 8) {
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

  const beginPaneDrag = (pane: "sidebar" | "inspector" | "terminal", event: React.PointerEvent) => {
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
    const saved = localStorage.getItem("yode-llm-providers");
    let models: string[] = [];
    if (saved) {
      try {
        const data = JSON.parse(saved);
        const list = Array.isArray(data) ? data : Object.values(data);
        const found = list.find((p: any) => p && p.id === provider);
        if (found && Array.isArray(found.models)) {
          models = found.models;
        }
      } catch (e) {}
    }
    if (models.length === 0) {
      const meta = PROVIDERS_META.find(p => p.id === provider);
      models = meta ? meta.defaultModels : [];
    }
    const lastModelKey = `yode-last-model-${provider}`;
    const lastUsedModel = localStorage.getItem(lastModelKey);
    const defaultModel = (lastUsedModel && models.includes(lastUsedModel)) ? lastUsedModel : (models[0] || "");

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
    localStorage.setItem(`yode-last-model-${currentProvider}`, model);

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
      const newLang = (e as CustomEvent).detail;
      setAppLang(newLang);
    };
    window.addEventListener("yode-language-change", handleLangChange);
    return () => window.removeEventListener("yode-language-change", handleLangChange);
  }, []);

  useEffect(() => {
    const handleGeneralSettingsChange = () => {
      setGeneralSettings(loadGeneralSettings());
      if ("__TAURI_INTERNALS__" in window) {
        invoke("general_settings_apply", { settings: loadGeneralSettingsPayload() }).catch(console.error);
      }
    };
    if ("__TAURI_INTERNALS__" in window) {
      invoke("general_settings_apply", { settings: loadGeneralSettingsPayload() }).catch(console.error);
    }
    window.addEventListener("yode-general-settings-change", handleGeneralSettingsChange);
    return () => window.removeEventListener("yode-general-settings-change", handleGeneralSettingsChange);
  }, []);

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
    localStorage.setItem(PROJECT_ROOTS_STORAGE_KEY, JSON.stringify(projectRoots));
  }, [projectRoots]);

  useEffect(() => {
    const handleProjectRootsChanged = () => {
      setProjectRoots(loadStoredProjectRoots());
      setProjectOrder(loadStoredProjectOrder());
    };
    window.addEventListener("yode-project-roots-changed", handleProjectRootsChanged);
    window.addEventListener("storage", handleProjectRootsChanged);
    return () => {
      window.removeEventListener("yode-project-roots-changed", handleProjectRootsChanged);
      window.removeEventListener("storage", handleProjectRootsChanged);
    };
  }, []);

  useEffect(() => {
    localStorage.setItem(PROJECT_ORDER_STORAGE_KEY, JSON.stringify(projectOrder));
  }, [projectOrder]);

  useEffect(() => {
    if (selectedProjectRoot === undefined) return;
    localStorage.setItem(
      SELECTED_PROJECT_ROOT_STORAGE_KEY,
      selectedProjectRoot === null ? STANDALONE_PROJECT_SENTINEL : selectedProjectRoot
    );
  }, [selectedProjectRoot]);

  // Load theme & settings on startup to avoid styling flashes
  useEffect(() => {
    const root = document.documentElement;

    // Mode
    const themeMode = localStorage.getItem("yode-theme-mode") || "dark";
    root.classList.remove("light", "dark");
    if (themeMode === "light") {
      root.classList.add("light");
      root.style.setProperty("color-scheme", "light");
    } else if (themeMode === "dark") {
      root.classList.add("dark");
      root.style.setProperty("color-scheme", "dark");
    } else {
      const isSystemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
      root.classList.add(isSystemDark ? "dark" : "light");
      root.style.setProperty("color-scheme", isSystemDark ? "dark" : "light");
    }

    // Colors & Fonts
    const accentColor = localStorage.getItem("yode-accent-color") || "#FF79C6";
    const backgroundColor = localStorage.getItem("yode-bg-color") || "#282A36";
    const foregroundColor = localStorage.getItem("yode-fg-color") || "#F8F8F2";
    const uiFont = localStorage.getItem("yode-ui-font") || "-apple-system, BlinkMacSystemFont, \"Segoe UI\", system-ui, sans-serif";
    const codeFont = localStorage.getItem("yode-code-font") || "ui-monospace, \"SF Mono\", SFMono-Regular, Menlo, Monaco, Consolas, monospace";
    const readNumber = (key: string, fallback: number) => {
      const raw = localStorage.getItem(key);
      if (raw === null) return fallback;
      const parsed = Number(raw);
      return Number.isFinite(parsed) ? parsed : fallback;
    };
    const appScale = readNumber("yode-app-scale", 100) / 100;
    const uiFontSize = readNumber("yode-ui-font-size", 13);
    const chatFontSize = readNumber("yode-chat-font-size", 13.25);
    const sidebarFontSize = readNumber("yode-sidebar-font-size", 13);
    const settingsFontSize = readNumber("yode-settings-font-size", 13);
    const codeFontSize = readNumber("yode-code-font-size", 12);
    const terminalFontSize = readNumber("yode-terminal-font-size", 12);
    const inspectorFontSize = readNumber("yode-inspector-font-size", 12);
    const contrast = localStorage.getItem("yode-contrast") || "48";
    const scaledPx = (value: number) => `${Number((value * appScale).toFixed(2))}px`;

    root.style.setProperty("--accent", accentColor);
    root.style.setProperty("--bg", backgroundColor);
    root.style.setProperty("--text", foregroundColor);
    root.style.setProperty("--font-ui", uiFont);
    root.style.setProperty("--font-code", codeFont);
    root.style.setProperty("--ui-font-size", scaledPx(uiFontSize));
    root.style.setProperty("--chat-font-size", scaledPx(chatFontSize));
    root.style.setProperty("--sidebar-font-size", scaledPx(sidebarFontSize));
    root.style.setProperty("--settings-font-size", scaledPx(settingsFontSize));
    root.style.setProperty("--code-font-size", scaledPx(codeFontSize));
    root.style.setProperty("--terminal-font-size", scaledPx(terminalFontSize));
    root.style.setProperty("--inspector-font-size", scaledPx(inspectorFontSize));
    root.style.setProperty("--app-scale", String(appScale));
    root.style.setProperty("--contrast-val", contrast);
    root.style.fontSize = scaledPx(uiFontSize);

    // Deriving colors based on background color lightness
    const hexToRgb = (hex: string) => {
      const shorthandRegex = /^#?([a-f\d])([a-f\d])([a-f\d])$/i;
      const fullHex = hex.replace(shorthandRegex, (_, r, g, b) => r + r + g + g + b + b);
      const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(fullHex);
      return result ? {
        r: parseInt(result[1], 16),
        g: parseInt(result[2], 16),
        b: parseInt(result[3], 16)
      } : null;
    };
    const rgbToHex = (r: number, g: number, b: number) => {
      const toHex = (c: number) => {
        const hex = Math.max(0, Math.min(255, c)).toString(16);
        return hex.length === 1 ? "0" + hex : hex;
      };
      return "#" + toHex(r) + toHex(g) + toHex(b);
    };
    const isLightColor = (hex: string) => {
      const rgb = hexToRgb(hex);
      if (!rgb) return false;
      const luminance = 0.299 * rgb.r + 0.587 * rgb.g + 0.114 * rgb.b;
      return luminance > 128;
    };
    const adjustBrightness = (hex: string, percent: number) => {
      const rgb = hexToRgb(hex);
      if (!rgb) return hex;
      const factor = 1 + (percent / 100);
      const r = Math.max(0, Math.min(255, Math.round(rgb.r * factor)));
      const g = Math.max(0, Math.min(255, Math.round(rgb.g * factor)));
      const b = Math.max(0, Math.min(255, Math.round(rgb.b * factor)));
      return rgbToHex(r, g, b);
    };

    const light = isLightColor(backgroundColor);
    const bgPercentMod = light ? -5 : 5;
    const bgDoubleMod = light ? -10 : 10;
    const bgTripleMod = light ? -15 : 15;
    const borderMod = light ? -18 : 18;
    const borderSoftMod = light ? -10 : 10;

    const chromeColor = adjustBrightness(backgroundColor, bgPercentMod);
    const panelColor = adjustBrightness(backgroundColor, bgDoubleMod);
    const panelRaised = adjustBrightness(backgroundColor, bgTripleMod);
    const fieldColor = adjustBrightness(backgroundColor, bgPercentMod);
    const lineColor = adjustBrightness(backgroundColor, borderMod);
    const lineSoftColor = adjustBrightness(backgroundColor, borderSoftMod);

    const rgbAccent = hexToRgb(accentColor);
    const accentMuted = rgbAccent ? `rgba(${rgbAccent.r}, ${rgbAccent.g}, ${rgbAccent.b}, 0.2)` : "rgba(255, 255, 255, 0.1)";

    root.style.setProperty("--chrome", chromeColor);
    root.style.setProperty("--panel", panelColor);
    root.style.setProperty("--panel-raised", panelRaised);
    root.style.setProperty("--field", fieldColor);
    root.style.setProperty("--line", lineColor);
    root.style.setProperty("--line-soft", lineSoftColor);
    root.style.setProperty("--accent-muted", accentMuted);

    // Pointer cursors
    if (localStorage.getItem("yode-use-pointers") === "true") {
      document.body.classList.add("use-pointers");
    }

    // Reduce Motion
    const reduceMotion = localStorage.getItem("yode-reduce-motion") || "system";
    if (reduceMotion === "on") {
      document.body.classList.add("reduce-motion");
    } else if (reduceMotion === "system") {
      const prefersReduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      if (prefersReduced) {
        document.body.classList.add("reduce-motion");
      }
    }

    // Font Smoothing
    const fontSmoothing = localStorage.getItem("yode-font-smoothing");
    if (fontSmoothing === null || fontSmoothing === "true") {
      document.body.classList.add("font-smoothing");
    } else {
      document.body.classList.add("no-font-smoothing");
    }
  }, []);

  useEffect(() => {
    // Sync translucent class to app-shell based on saved value
    const val = localStorage.getItem("yode-translucent-sidebar");
    const isTranslucent = val === null ? true : val === "true";
    const shells = document.querySelectorAll(".app-shell");
    shells.forEach(shell => {
      if (isTranslucent) {
        shell.classList.add("translucent-sidebar");
        shell.classList.remove("translucent-sidebar-disabled");
      } else {
        shell.classList.remove("translucent-sidebar");
        shell.classList.add("translucent-sidebar-disabled");
      }
    });
  }, [viewMode]);

  const loadBootstrap = () => {
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
              if (activeSessionIdRef.current === activeSessionId) {
                setTimelineItems(messagesToTimelineItems(messages));
              }
            })
            .catch(console.error);
        }
      })
      .catch(() => {
        setBootstrap(fallbackBootstrap);
        if (!("__TAURI_INTERNALS__" in window)) {
          const activeSessions = visibleSessions(sessions);
          const activeSessionId = activeSessions.find((session) => session.active)?.id ?? null;

          setSessionItems(activeSessions);
          activeSessionIdRef.current = activeSessionId;
          setActiveSessionId(activeSessionId);
          setSelectedProjectRoot((current) =>
            current === undefined ? fallbackBootstrap.workspacePath : current
          );
          setTimelineItems(timeline);
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
      const detail = (event as CustomEvent<{ provider?: string; model?: string }>).detail;
      if (!detail?.provider || !detail?.model) {
        loadBootstrap();
        return;
      }
      setBootstrap((current) => ({
        ...current,
        provider: detail.provider!,
        model: detail.model!
      }));
    };
    const handlePermanentDelete = (event: Event) => {
      const sessionId = (event as CustomEvent<{ sessionId?: string }>).detail?.sessionId;
      if (!sessionId) {
        loadBootstrap();
        return;
      }
      setSessionItems((items) => items.filter((session) => session.id !== sessionId));
      setActiveSessionId((current) => current === sessionId ? null : current);
    };
    window.addEventListener("yode-session-unarchived", handleUnarchive);
    window.addEventListener("yode-default-llm-change", handleDefaultLlmChange);
    window.addEventListener("yode-session-deleted-permanently", handlePermanentDelete);
    window.addEventListener("yode-sessions-imported", handleUnarchive);
    return () => {
      window.removeEventListener("yode-session-unarchived", handleUnarchive);
      window.removeEventListener("yode-default-llm-change", handleDefaultLlmChange);
      window.removeEventListener("yode-session-deleted-permanently", handlePermanentDelete);
      window.removeEventListener("yode-sessions-imported", handleUnarchive);
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
      const payload = event.payload;
      const outer = (payload as any).kind ? (payload as DesktopEvent) : null;
      const kind = outer ? outer.kind : (event as any).kind;
      const eventSessionId = outer?.sessionId ?? (payload as any).sessionId;
      if (
        eventSessionId &&
        activeSessionIdRef.current &&
        eventSessionId !== activeSessionIdRef.current
      ) {
        return;
      }

      if (kind === "turn_started") {
        setIsProcessing(true);
        if (outer) {
          setCurrentTurnId(outer.turnId);
        }
      } else if (kind === "ask_user" && eventSessionId && (outer?.turnId ?? (payload as any).turnId)) {
        sendSystemNotification("Yode 需要你的回复", String((payload as any).payload?.body ?? "任务正在等待输入。"), "question");
        setPendingUserQuestion({
          sessionId: eventSessionId,
          turnId: outer?.turnId ?? (payload as any).turnId,
          question: String((payload as any).payload?.body ?? "请回复问题")
        });
      } else if (kind === "tool_confirm_required" || kind === "permission") {
        sendSystemNotification("Yode 请求执行权限", String((payload as any).payload?.body ?? "有操作需要确认。"), "permission");
      } else if (kind === "turn_completed" || kind === "error") {
        setIsProcessing(false);
        setPendingUserQuestion(null);
        if (kind === "turn_completed") {
          sendSystemNotification("Yode 已完成任务", String((payload as any).payload?.body ?? "本轮运行已完成。").slice(0, 160), "completion");
        }
      }

      setTimelineItems((items) =>
        applyDesktopEventToTimelineItems(
          items,
          (payload as any).kind ? (payload as DesktopEvent) : payload,
          (payload as any).kind ? undefined : (event as any).kind
        )
      );
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
    };
  }, [sendSystemNotification]);

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
    setActiveSessionId(null);
    setCurrentTurnId(null);
    setMessageQueue([]);
    setIsProcessing(false);
    setPendingUserQuestion(null);
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

  async function handleAskUserResolve(answer: string) {
    if (!pendingUserQuestion) return;
    
    let displayText = answer;
    try {
      const parsed = JSON.parse(answer);
      displayText = Object.values(parsed).join(", ");
    } catch (e) {}

    setTimelineItems((items) => [
      ...items,
      {
        id: `ask-answer-${Date.now()}`,
        kind: "user",
        title: "用户",
        body: displayText,
        createdAt: Date.now()
      }
    ]);

    await invoke("ask_user_respond", {
      sessionId: pendingUserQuestion.sessionId,
      session_id: pendingUserQuestion.sessionId,
      turnId: pendingUserQuestion.turnId,
      turn_id: pendingUserQuestion.turnId,
      answer: answer
    }).catch((err) => {
      console.error(err);
      setTimelineItems((items) => [
        ...items,
        {
          id: `ask-answer-error-${Date.now()}`,
          kind: "assistant",
          title: "错误",
          body: "发送问题回复失败。",
          meta: "stream complete"
        }
      ]);
    });

    setPendingUserQuestion(null);
  }

  async function handleSendMessage() {
    if (!draft.trim() && composerImages.length === 0) return;
    const content = draft.trim();
    const imagesAtSend = composerImages;

    if (pendingUserQuestion) {
      setDraft("");
      setComposerImages([]);
      setTimelineItems((items) => [
        ...items,
        {
          id: `ask-answer-${Date.now()}`,
          kind: "user",
          title: "用户",
          body: content,
          createdAt: Date.now()
        }
      ]);
      await invoke("ask_user_respond", {
        sessionId: pendingUserQuestion.sessionId,
        session_id: pendingUserQuestion.sessionId,
        turnId: pendingUserQuestion.turnId,
        turn_id: pendingUserQuestion.turnId,
        answer: content
      }).catch((err) => {
        console.error(err);
        setTimelineItems((items) => [
          ...items,
          {
            id: `ask-answer-error-${Date.now()}`,
            kind: "assistant",
            title: "错误",
            body: "发送问题回复失败。",
            meta: "stream complete"
          }
        ]);
      });
      setPendingUserQuestion(null);
      return;
    }

    const isDetachedReview = generalSettings.codeReviewPolicy === "detached" && /^\/review\b/i.test(content);
    const sessionIdAtSend = isDetachedReview ? null : (activeSession?.id ?? null);
    const projectRootAtSend = selectedProjectRoot === undefined ? bootstrap.workspacePath : selectedProjectRoot;
    setDraft("");
    setComposerImages([]);

    if (isProcessing) {
      if (generalSettings.followUpBehavior === "steer") {
        setTimelineItems((items) => [
          ...items,
          {
            id: `local-steer-${Date.now()}`,
            kind: "assistant",
            title: "指引已记录",
            body: content,
            meta: "intermediate",
            createdAt: Date.now()
          }
        ]);
        return;
      }
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

    setIsProcessing(true);
    setTimelineItems((items) => [
      ...items,
      {
        id: `local-${Date.now()}`,
        kind: "user",
        title: "用户",
        body: content,
        attachments: imagesAtSend,
        createdAt: Date.now()
      }
    ]);

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
      setCurrentTurnId(res.turnId);
      activeSessionIdRef.current = res.sessionId;
      setActiveSessionId(res.sessionId);
      setSessionItems((items) => upsertActiveSession(items, res.session));
    } catch (err) {
      console.error(err);
      setIsProcessing(false);
      setDraft(content);
      setComposerImages(imagesAtSend);
    }
  }

  useEffect(() => {
    if (!isProcessing && messageQueue.length > 0 && activeSession?.id) {
      const nextMessage = messageQueue[0];
      const nextContent = nextMessage.content;
      setMessageQueue((prev) => prev.slice(1));
      setIsProcessing(true);
      
      setTimelineItems((items) =>
        items.map((item) =>
          item.kind === "user" && item.body === nextContent && item.title.includes("等待中")
            ? { ...item, title: "用户" }
            : item
        )
      );

      invoke<TurnAccepted>("turn_send_message", {
        request: {
          sessionId: activeSession.id,
          content: nextContent,
          images: nextMessage.images.map(imageToRequestPayload),
          projectRoot: undefined,
          standalone: undefined,
          title: undefined,
          provider: undefined,
          model: undefined
        }
      }).then((res) => {
        setCurrentTurnId(res.turnId);
        activeSessionIdRef.current = res.sessionId;
        setSessionItems((items) => upsertActiveSession(items, res.session));
      }).catch((err) => {
        console.error(err);
        setIsProcessing(false);
      });
    }
  }, [isProcessing, messageQueue, activeSession?.id]);

  async function handleCancelMessage() {
    if (activeSession?.id && currentTurnId) {
      await invoke("turn_cancel", {
        sessionId: activeSession.id,
        turnId: currentTurnId
      }).catch(console.error);

      setTimelineItems((items) => [
        ...items,
        {
          id: `cancel-${currentTurnId}-${Date.now()}`,
          kind: "boundary",
          title: "已手动终止",
          body: "用户已取消此轮运行。",
          createdAt: Date.now()
        }
      ]);

      setIsProcessing(false);
    }
  }

  async function handleSelectSession(sessionId: string) {
    const nextSession = sessionItems.find((item) => item.id === sessionId);
    activeSessionIdRef.current = sessionId;
    setActiveSessionId(sessionId);
    setSelectedProjectRoot(nextSession?.projectRoot ?? null);
    setIsProcessing(false);
    setCurrentTurnId(null);
    setMessageQueue([]);
    setPendingUserQuestion(null);

    if (!("__TAURI_INTERNALS__" in window)) {
      setTimelineItems(timeline);
      return;
    }

    try {
      const messages = await invoke<DesktopMessage[]>("sessions_messages", {
        sessionId,
        session_id: sessionId
      });
      if (activeSessionIdRef.current !== sessionId) return;
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
    localStorage.setItem("yode-view-mode", mode);
  };

  const handleDeleteSession = (sessionId: string) => {
    const session = sessionItems.find(s => s.id === sessionId);
    if (!session) return;

    // 1. Get and update yode-archived-session-ids
    const savedIds = localStorage.getItem(ARCHIVED_SESSION_IDS_STORAGE_KEY);
    let archivedIds: string[] = [];
    if (savedIds) {
      try {
        archivedIds = JSON.parse(savedIds);
      } catch (e) {}
    }
    if (!archivedIds.includes(sessionId)) {
      archivedIds.push(sessionId);
    }
    localStorage.setItem(ARCHIVED_SESSION_IDS_STORAGE_KEY, JSON.stringify(archivedIds));

    // 2. Get and update yode-archived-chats
    const savedChats = localStorage.getItem("yode-archived-chats");
    let archivedChats: any[] = [];
    if (savedChats) {
      try {
        archivedChats = JSON.parse(savedChats);
      } catch (e) {}
    }
    if (!archivedChats.some(c => c.id === sessionId)) {
      archivedChats.push({
        id: sessionId,
        title: session.title,
        date: session.updatedAt,
        project: session.project || "default"
      });
    }
    localStorage.setItem("yode-archived-chats", JSON.stringify(archivedChats));

    // 3. Filter state
    setSessionItems(prev => prev.filter(s => s.id !== sessionId));
    if (activeSessionIdRef.current === sessionId) {
      const nextProjectRoot = session.projectRoot ?? null;
      activeSessionIdRef.current = null;
      setActiveSessionId(null);
      setCurrentTurnId(null);
      setMessageQueue([]);
      setIsProcessing(false);
      setPendingUserQuestion(null);
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
          localStorage.setItem("yode-active-tab", "键盘快捷键");
          break;
        case "toggle_sidebar":
          event.preventDefault();
          setSidebarOpen((open) => !open);
          break;
        case "toggle_side_panel":
          event.preventDefault();
          setInspectorOpen((open) => !open);
          break;
        case "open_terminal":
          event.preventDefault();
          setTerminalOpenForCurrentConversation(!terminalOpen);
          break;
        case "toggle_bottom_panel": {
          event.preventDefault();
          const next = localStorage.getItem("yode-bottom-panel") === "false";
          localStorage.setItem("yode-bottom-panel", String(next));
          window.dispatchEvent(new Event("yode-general-settings-change"));
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
    window.addEventListener("yode-keyboard-shortcuts-change", refreshBindings);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("yode-keyboard-shortcuts-change", refreshBindings);
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
