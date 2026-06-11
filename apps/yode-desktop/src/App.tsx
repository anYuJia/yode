import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Archive,
  Bot,
  ChevronDown,
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
  Trash2
} from "lucide-react";
import React, { useEffect, useLayoutEffect, useMemo, useState, useRef } from "react";
import { createPortal } from "react-dom";

import {
  Bootstrap,
  DesktopEvent,
  DesktopMessage,
  fallbackBootstrap,
  SessionSummary,
  sessions,
  TimelineItem,
  timeline,
  TurnAccepted
} from "./lib/mock";
import { SettingsShell } from "./components/SettingsShell";
import { TerminalDrawer } from "./components/TerminalDrawer";
import { PROVIDERS_META } from "./components/settings/ProvidersSettings";

type ViewMode = "chat" | "settings";
type PendingUserQuestion = {
  sessionId: string;
  turnId: string;
  question: string;
};

const PROJECT_ROOTS_STORAGE_KEY = "yode-project-roots";
const PROJECT_ORDER_STORAGE_KEY = "yode-project-order";
const SELECTED_PROJECT_ROOT_STORAGE_KEY = "yode-selected-project-root";
const ARCHIVED_SESSION_IDS_STORAGE_KEY = "yode-archived-session-ids";
const DELETED_SESSION_IDS_STORAGE_KEY = "yode-deleted-session-ids";
const STANDALONE_PROJECT_SENTINEL = "__standalone__";

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
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [messageQueue, setMessageQueue] = useState<string[]>([]);
  const [currentTurnId, setCurrentTurnId] = useState<string | null>(null);
  const [permissionMode, setPermissionMode] = useState<string>("default");
  const [pendingUserQuestion, setPendingUserQuestion] = useState<PendingUserQuestion | null>(null);
  const activeSessionIdRef = useRef<string | null>(null);

  useEffect(() => {
    activeSessionIdRef.current = activeSessionId;
  }, [activeSessionId]);

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
    localStorage.setItem(PROJECT_ROOTS_STORAGE_KEY, JSON.stringify(projectRoots));
  }, [projectRoots]);

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
    const codeFontSize = localStorage.getItem("yode-code-font-size") || "12";
    const contrast = localStorage.getItem("yode-contrast") || "48";
    const uiFontSize = localStorage.getItem("yode-ui-font-size") || "13";

    root.style.setProperty("--accent", accentColor);
    root.style.setProperty("--bg", backgroundColor);
    root.style.setProperty("--text", foregroundColor);
    root.style.setProperty("--font-ui", uiFont);
    root.style.setProperty("--font-code", codeFont);
    root.style.setProperty("--code-font-size", `${codeFontSize}px`);
    root.style.setProperty("--contrast-val", contrast);
    root.style.fontSize = `${uiFontSize}px`;

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
    window.addEventListener("yode-session-deleted-permanently", handlePermanentDelete);
    return () => {
      window.removeEventListener("yode-session-unarchived", handleUnarchive);
      window.removeEventListener("yode-session-deleted-permanently", handlePermanentDelete);
    };
  }, []);

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) {
      return;
    }

    let unlisten: (() => void) | undefined;
    listen<DesktopEvent>("desktop-event", (event) => {
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
        setPendingUserQuestion({
          sessionId: eventSessionId,
          turnId: outer?.turnId ?? (payload as any).turnId,
          question: String((payload as any).payload?.body ?? "请回复问题")
        });
      } else if (kind === "turn_completed" || kind === "error") {
        setIsProcessing(false);
        setPendingUserQuestion(null);
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
        unlisten = dispose;
      })
      .catch(console.error);

    return () => unlisten?.();
  }, []);

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

  async function handleSendMessage() {
    if (!draft.trim()) return;
    const content = draft.trim();

    if (pendingUserQuestion) {
      setDraft("");
      setTimelineItems((items) => [
        ...items,
        {
          id: `ask-answer-${Date.now()}`,
          kind: "user",
          title: "用户",
          body: content
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

    const sessionIdAtSend = activeSession?.id ?? null;
    const projectRootAtSend = selectedProjectRoot === undefined ? bootstrap.workspacePath : selectedProjectRoot;
    setDraft("");

    if (isProcessing) {
      setMessageQueue((prev) => [...prev, content]);
      setTimelineItems((items) => [
        ...items,
        {
          id: `local-queued-${Date.now()}`,
          kind: "user",
          title: "用户 (等待中...)",
          body: content
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
        body: content
      }
    ]);
    try {
      const res = await invoke<TurnAccepted>("turn_send_message", {
        request: {
          sessionId: sessionIdAtSend,
          content,
          projectRoot: sessionIdAtSend ? undefined : projectRootAtSend,
          standalone: sessionIdAtSend ? undefined : projectRootAtSend === null,
          title: sessionIdAtSend ? undefined : deriveSessionTitle(content),
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
    }
  }

  useEffect(() => {
    if (!isProcessing && messageQueue.length > 0 && activeSession?.id) {
      const nextContent = messageQueue[0];
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

  if (viewMode === "settings") {
    return (
      <main className="app-shell" style={{ display: "block", width: "100vw", height: "100vh", overflow: "hidden" }}>
        <SettingsShell bootstrap={bootstrap} onClose={() => handleSetViewMode("chat")} />
      </main>
    );
  }

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
  };

  const isStandalone = activeSession
    ? !activeSession.projectRoot
    : selectedProjectRoot === null;

  const displayedWorkspacePath = isStandalone
    ? null
    : (activeSession?.projectRoot ?? selectedProjectRoot ?? bootstrap.workspacePath);

  return (
    <main className="app-shell">
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
      <section className="workspace" style={{ position: "relative", overflow: "hidden" }}>
        <Topbar
          bootstrap={bootstrap}
          sessionTitle={activeSession?.title ?? (appLang === "zh" ? "新对话" : "New chat")}
          workspacePath={displayedWorkspacePath}
          inspectorOpen={inspectorOpen}
          isProcessing={isProcessing && !pendingUserQuestion}
          onToggleInspector={() => setInspectorOpen(!inspectorOpen)}
          terminalOpen={terminalOpen}
          onToggleTerminal={() => setTerminalOpen(!terminalOpen)}
          currentProvider={currentProvider}
          currentModel={currentModel}
          onProviderChange={handleUpdateProvider}
          onModelChange={handleUpdateModel}
        />
        <ChatWorkspace
          draft={draft}
          timelineItems={timelineItems}
          onDraftChange={setDraft}
          onSendMessage={handleSendMessage}
          inspectorOpen={inspectorOpen}
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
        />
        <TerminalDrawer isOpen={terminalOpen} onClose={() => setTerminalOpen(false)} />
      </section>
    </main>
  );
}
function Sidebar({
  sessions,
  projectOptions,
  activeSessionId,
  viewMode,
  onChangeView,
  onCreateSession,
  onSelectSession,
  onAddProject,
  onProjectReorder,
  onDeleteSession
}: {
  sessions: SessionSummary[];
  projectOptions: Array<{ label: string; root: string | null }>;
  activeSessionId: string | null;
  viewMode: ViewMode;
  onChangeView: (mode: ViewMode) => void;
  onCreateSession: (projectRoot?: string | null) => void;
  onSelectSession: (sessionId: string) => void;
  onAddProject: () => Promise<void>;
  onProjectReorder: (draggedRoot: string, targetRoot: string, placement?: "before" | "after") => void;
  onDeleteSession: (sessionId: string) => void;
}) {
  const lang = localStorage.getItem("yode-language") || "zh";
  const isZh = lang === "zh";
  const t = (zhText: string, enText: string) => isZh ? zhText : enText;

  const [pinnedSessionIds, setPinnedSessionIds] = useState<string[]>(["s-1"]);
  const [deletingSessionId, setDeletingSessionId] = useState<string | null>(null);
  const [expandedProjectIds, setExpandedProjectIds] = useState<string[]>([]);
  const [draggingProjectId, setDraggingProjectId] = useState<string | null>(null);
  const [dragGhost, setDragGhost] = useState<{
    name: string;
    count: number;
    sessions: SessionSummary[];
    expanded: boolean;
    left: number;
    width: number;
    height: number;
    y: number;
  } | null>(null);
  
  // Hover information popover state
  const [hoveredSessionId, setHoveredSessionId] = useState<string | null>(null);
  const [hoverPosition, setHoverPosition] = useState<{ top: number; left: number } | null>(null);
  const hoverTimerRef = useRef<number | null>(null);
  const projectGroupsRef = useRef<Array<{ id: string; name: string; sessions: SessionSummary[] }>>([]);
  const projectNodeRefs = useRef(new Map<string, HTMLDivElement>());
  const projectFlipRectsRef = useRef(new Map<string, DOMRect>());
  const knownProjectIdsRef = useRef(new Set<string>());
  const dragStateRef = useRef<{
    id: string;
    name: string;
    count: number;
    sessions: SessionSummary[];
    expanded: boolean;
    left: number;
    width: number;
    height: number;
    offsetY: number;
    startY: number;
    hasMoved: boolean;
  } | null>(null);
  const suppressProjectClickRef = useRef(false);

  const handleMouseEnter = (sessionId: string, e: React.MouseEvent) => {
    // Clear any active timer
    if (hoverTimerRef.current) window.clearTimeout(hoverTimerRef.current);
    
    const rect = e.currentTarget.getBoundingClientRect();
    const pos = {
      top: rect.top,
      left: 240
    };

    hoverTimerRef.current = window.setTimeout(() => {
      setHoveredSessionId(sessionId);
      setHoverPosition(pos);
    }, 600);
  };

  const handleMouseLeave = () => {
    if (hoverTimerRef.current) {
      window.clearTimeout(hoverTimerRef.current);
      hoverTimerRef.current = null;
    }
    setHoveredSessionId(null);
    setHoverPosition(null);
  };

  useEffect(() => {
    return () => {
      if (hoverTimerRef.current) window.clearTimeout(hoverTimerRef.current);
    };
  }, []);

  const handleTogglePin = (sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setPinnedSessionIds(prev => 
      prev.includes(sessionId) 
        ? prev.filter(id => id !== sessionId) 
        : [...prev, sessionId]
    );
  };

  const handleDeleteClick = (sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setDeletingSessionId(sessionId);
  };

  const handleConfirmDelete = (sessionId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    // execute delete
    onDeleteSession(sessionId);
    setDeletingSessionId(null);
  };

  const handleSessionMouseLeave = (sessionId: string) => {
    // If the mouse leaves the session button, cancel deletion mode
    if (deletingSessionId === sessionId) {
      setDeletingSessionId(null);
    }
    handleMouseLeave();
  };

  const { projectGroups, standaloneSessions } = useMemo(() => {
    const groupMap = new Map<string, SessionSummary[]>();
    const standalone: SessionSummary[] = [];

    sessions.forEach((session) => {
      const projectRoot = session.projectRoot?.trim();
      if (!projectRoot) {
        standalone.push(session);
        return;
      }
      const existing = groupMap.get(projectRoot) ?? [];
      existing.push(session);
      groupMap.set(projectRoot, existing);
    });

    const sortSessions = (items: SessionSummary[]) =>
      [...items].sort((a, b) => {
        const pinDelta = Number(pinnedSessionIds.includes(b.id)) - Number(pinnedSessionIds.includes(a.id));
        return pinDelta || 0;
      });

    return {
      projectGroups: projectOptions
        .filter((option) => option.root)
        .map((option) => ({
          id: option.root!,
          name: option.label,
          sessions: sortSessions(groupMap.get(option.root!) ?? [])
        })),
      standaloneSessions: sortSessions(standalone)
    };
  }, [pinnedSessionIds, projectOptions, sessions]);

  projectGroupsRef.current = projectGroups;
  const projectLayoutKey = useMemo(
    () => projectGroups.map((group) => group.id).join("\n"),
    [projectGroups]
  );

  useLayoutEffect(() => {
    const previousRects = projectFlipRectsRef.current;
    const nextRects = new Map<string, DOMRect>();

    projectGroupsRef.current.forEach((group) => {
      const node = projectNodeRefs.current.get(group.id);
      if (!node) return;
      const nextRect = node.getBoundingClientRect();
      nextRects.set(group.id, nextRect);
      if (group.id === draggingProjectId) return;
      const previousRect = previousRects.get(group.id);
      if (!previousRect) return;
      const deltaY = previousRect.top - nextRect.top;
      if (Math.abs(deltaY) < 0.5) return;
      if (document.body.classList.contains("reduce-motion")) return;
      node.animate(
        [
          { transform: `translateY(${deltaY}px)` },
          { transform: "translateY(0)" }
        ],
        {
          duration: 260,
          easing: "cubic-bezier(0.16, 1, 0.3, 1)"
        }
      );
    });

    projectFlipRectsRef.current = nextRects;
  }, [projectLayoutKey, draggingProjectId]);

  useEffect(() => {
    const currentProjectGroups = projectGroupsRef.current;
    const nextKnownProjectIds = new Set(currentProjectGroups.map((group) => group.id));
    const newlyDiscoveredProjectIds = currentProjectGroups
      .filter((group) => !knownProjectIdsRef.current.has(group.id))
      .map((group) => group.id);
    knownProjectIdsRef.current = nextKnownProjectIds;

    setExpandedProjectIds((current) => {
      const kept = current.filter((id) => nextKnownProjectIds.has(id));
      const next = [
        ...kept,
        ...newlyDiscoveredProjectIds.filter((id) => !kept.includes(id))
      ];
      return next;
    });
  }, [projectLayoutKey]);

  // Helper render method for a session item
  const renderSessionItem = (session: SessionSummary) => {
    const isPinned = pinnedSessionIds.includes(session.id);
    const isDeleting = deletingSessionId === session.id;
    const isActive = session.id === activeSessionId;

    return (
      <div
        className={`session-item-wrapper ${isActive ? "active" : ""}`}
        key={session.id}
        onMouseEnter={(e) => handleMouseEnter(session.id, e)}
        onMouseLeave={() => handleSessionMouseLeave(session.id)}
        style={{ position: "relative" }}
      >
        <button
          className={`session-button ${isActive ? "active" : ""}`}
          onClick={() => onSelectSession(session.id)}
          type="button"
        >
          <span className="session-title">
            {session.title}
          </span>
          {!isDeleting && (
            <span className="session-time" style={{ fontSize: "10.5px", color: "var(--text-soft)", marginLeft: "4px" }}>
              {session.updatedAt}
            </span>
          )}
        </button>

        {isDeleting ? (
          <div className="delete-confirm-overlay">
            <button
              onClick={(e) => handleConfirmDelete(session.id, e)}
              type="button"
              className="confirm-delete-btn"
            >
              {t("确认", "Confirm")}
            </button>
          </div>
        ) : (
          <div className="session-actions-overlay">
            <button
              onClick={(e) => handleTogglePin(session.id, e)}
              type="button"
              className="action-icon-btn"
              title={isPinned ? t("取消置顶", "Unpin") : t("置顶", "Pin")}
            >
              <Pin size={13} style={{ transform: isPinned ? "rotate(45deg)" : "none" }} />
            </button>
            <button
              onClick={(e) => handleDeleteClick(session.id, e)}
              type="button"
              className="action-icon-btn"
              title={t("删除", "Delete")}
            >
              <Trash2 size={13} />
            </button>
          </div>
        )}
      </div>
    );
  };

  const beginProjectPointerTracking = (
    group: { id: string; name: string; sessions: SessionSummary[] },
    event: React.PointerEvent<HTMLButtonElement>
  ) => {
    if (event.button !== 0) return;
    const groupNode = projectNodeRefs.current.get(group.id);
    const rect = (groupNode ?? event.currentTarget).getBoundingClientRect();
    const isExpandedAtStart = expandedProjectIds.includes(group.id);
    dragStateRef.current = {
      id: group.id,
      name: group.name,
      count: group.sessions.length,
      sessions: group.sessions,
      expanded: isExpandedAtStart,
      left: rect.left,
      width: rect.width,
      height: rect.height,
      offsetY: event.clientY - rect.top,
      startY: event.clientY,
      hasMoved: false
    };

    const handlePointerMove = (moveEvent: PointerEvent) => {
      const dragState = dragStateRef.current;
      if (!dragState) return;
      const moved = Math.abs(moveEvent.clientY - dragState.startY) > 4;
      if (!dragState.hasMoved) {
        if (!moved) return;
        dragState.hasMoved = true;
        suppressProjectClickRef.current = true;
        setDraggingProjectId(dragState.id);
        setDragGhost({
          name: dragState.name,
          count: dragState.count,
          sessions: dragState.sessions,
          expanded: dragState.expanded,
          left: dragState.left,
          width: dragState.width,
          height: dragState.height,
          y: moveEvent.clientY - dragState.offsetY
        });
      }

      moveEvent.preventDefault();
      setDragGhost((current) =>
        current ? { ...current, y: moveEvent.clientY - dragState.offsetY } : current
      );

      const groups = projectGroupsRef.current.filter((item) => item.id !== dragState.id);
      if (groups.length === 0) return;

      let targetId = groups[groups.length - 1].id;
      let placement: "before" | "after" = "after";
      for (const item of groups) {
        const node = projectNodeRefs.current.get(item.id);
        if (!node) continue;
        const itemRect = node.getBoundingClientRect();
        if (moveEvent.clientY < itemRect.top + itemRect.height / 2) {
          targetId = item.id;
          placement = "before";
          break;
        }
      }
      onProjectReorder(dragState.id, targetId, placement);
    };

    const finishPointerTracking = () => {
      dragStateRef.current = null;
      setDraggingProjectId(null);
      setDragGhost(null);
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", finishPointerTracking);
      window.removeEventListener("pointercancel", finishPointerTracking);
      window.setTimeout(() => {
        suppressProjectClickRef.current = false;
      }, 0);
    };

    window.addEventListener("pointermove", handlePointerMove, { passive: false });
    window.addEventListener("pointerup", finishPointerTracking);
    window.addEventListener("pointercancel", finishPointerTracking);
  };

  const renderProjectGroup = (group: { id: string; name: string; sessions: SessionSummary[] }) => {
    const expanded = expandedProjectIds.includes(group.id);
    const hasActiveSession = group.sessions.some((session) => session.id === activeSessionId);
    const isDragging = draggingProjectId === group.id;

    const style: React.CSSProperties = {
      position: "relative",
      zIndex: isDragging ? 10 : 1
    };

    return (
      <div
        className={`project-group ${hasActiveSession ? "active" : ""} ${isDragging ? "dragging" : ""}`}
        key={group.id}
        ref={(node) => {
          if (node) {
            projectNodeRefs.current.set(group.id, node);
          } else {
            projectNodeRefs.current.delete(group.id);
          }
        }}
        style={style}
      >
      <div className="project-header-wrapper" style={{ position: "relative" }}>
        <button
          className={`project-button ${hasActiveSession ? "active" : ""}`}
          onPointerDown={(event) => {
            beginProjectPointerTracking(group, event);
          }}
          onClick={(event) => {
            if (suppressProjectClickRef.current) {
              event.preventDefault();
              return;
            }
            setExpandedProjectIds((current) =>
              current.includes(group.id)
                ? current.filter((id) => id !== group.id)
                : [...current, group.id]
            );
          }}
          type="button"
        >
          <Folder size={16} />
          <span>
            {group.name}
            <em>{group.sessions.length}</em>
          </span>
          <ChevronDown className={expanded ? "expanded" : ""} size={15} />
        </button>
        <div className="project-actions-overlay">
          <button
            onClick={(e) => {
              e.stopPropagation();
              onCreateSession(group.id);
            }}
            type="button"
            className="action-icon-btn"
            title={t("新建对话", "New chat")}
          >
            <Plus size={13} />
          </button>
        </div>
      </div>
        <div
          className={`project-sessions-shell ${expanded ? "expanded" : "collapsed"}`}
          aria-hidden={!expanded}
        >
          <div className="project-sessions-inner">
            <div className="project-sessions">
              {group.sessions.map(renderSessionItem)}
            </div>
            {group.sessions.length === 0 ? (
              <div className="project-empty">{t("暂无会话", "No chats yet")}</div>
            ) : null}
          </div>
        </div>
      </div>
    );
  };

  return (
    <aside className="sidebar" style={{ position: "relative" }}>
      <div className="brand-row" data-tauri-drag-region>
        <div className="brand-mark">Y</div>
        <div data-tauri-drag-region>
          <div className="brand-title" data-tauri-drag-region>Yode</div>
          <div className="brand-subtitle" data-tauri-drag-region>local agent runtime</div>
        </div>
      </div>

      <button className="primary-action" onClick={() => onCreateSession()} type="button">
        <MessageSquarePlus size={17} />
        {t("新对话", "New chat")}
      </button>

      <nav className="nav-block" aria-label="主导航">
        <NavButton icon={<Search size={16} />} label={t("搜索", "Search")} />
        <NavButton icon={<Code2 size={16} />} label={t("技能", "Skills")} />
        <NavButton icon={<Workflow size={16} />} label={t("插件", "Plugins")} />
        <NavButton icon={<Clock3 size={16} />} label={t("自动化", "Autopilot")} />
      </nav>

      <div className="sidebar-section sessions">
        <div className="section-head">
          <div className="section-label">{t("项目与对话", "Projects & Chats")}</div>
          <button className="section-action" type="button" onClick={() => void onAddProject()}>
            <FolderPlus size={14} />
            {t("添加项目", "Add project")}
          </button>
        </div>
        <div className="sessions-list">
          {projectGroups.map(renderProjectGroup)}
          {standaloneSessions.length > 0 ? (
            <div className="standalone-group">
              <div className="standalone-label">{t("独立对话", "Standalone")}</div>
              {standaloneSessions.map(renderSessionItem)}
            </div>
          ) : null}
        </div>
      </div>

      {/* Hover info popover card */}
      {hoveredSessionId && hoverPosition && createPortal(
        <div
          className="session-popover"
          style={{
            position: "fixed",
            top: hoverPosition.top,
            left: hoverPosition.left,
            zIndex: 9999,
            width: "220px",
            background: "var(--panel-raised)",
            border: "1px solid var(--line)",
            borderRadius: "var(--radius)",
            padding: "10px",
            boxShadow: "var(--shadow-raised)",
            color: "var(--text)",
            pointerEvents: "none",
            animation: "fadeIn 0.15s ease-out"
          }}
        >
          {(() => {
            const s = sessions.find(x => x.id === hoveredSessionId);
            if (!s) return null;
            return (
              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <div style={{ fontSize: "12px", fontWeight: "700", color: "var(--accent)" }}>
                  {s.title}
                </div>
                <div style={{ display: "flex", flexDirection: "column", gap: "3px", fontSize: "10.5px", color: "var(--text-muted)" }}>
                  <div>
                    <span style={{ color: "var(--text-soft)" }}>{t("项目：", "Project: ")}</span>
                    <code>{s.project || (s.projectRoot ? projectLabelFromPath(s.projectRoot) : t("独立对话", "Standalone"))}</code>
                  </div>
                  <div>
                    <span style={{ color: "var(--text-soft)" }}>{t("更新时间：", "Updated: ")}</span>
                    {s.updatedAt}
                  </div>
                  <div>
                    <span style={{ color: "var(--text-soft)" }}>{t("会话 ID：", "Session ID: ")}</span>
                    <span style={{ fontFamily: "var(--font-code)", opacity: 0.8 }}>{s.id}</span>
                  </div>
                </div>
              </div>
            );
          })()}
        </div>,
        document.body
      )}

      {dragGhost && createPortal(
        <div
          className={`project-drag-ghost ${dragGhost.expanded ? "expanded" : ""}`}
          style={{
            left: dragGhost.left,
            top: dragGhost.y,
            width: dragGhost.width,
            height: dragGhost.height
          }}
        >
          <div className="project-drag-ghost-head">
            <Folder size={16} />
            <span>
              {dragGhost.name}
              <em>{dragGhost.count}</em>
            </span>
          </div>
          {dragGhost.expanded ? (
            <div className="project-drag-ghost-sessions">
              {dragGhost.sessions.length > 0 ? (
                dragGhost.sessions.map((session) => (
                  <div className="project-drag-ghost-session" key={session.id}>
                    <span>{session.title}</span>
                    <em>{session.updatedAt}</em>
                  </div>
                ))
              ) : (
                <div className="project-drag-ghost-empty">{t("暂无会话", "No chats yet")}</div>
              )}
            </div>
          ) : null}
        </div>,
        document.body
      )}

      <div className="sidebar-footer">
        <button
          className={`footer-button ${viewMode === "settings" ? "active" : ""}`}
          onClick={() => onChangeView("settings")}
          type="button"
          title={t("设置", "Settings")}
        >
          <Settings size={17} />
          {t("设置", "Settings")}
        </button>
      </div>
    </aside>
  );
}


function NavButton({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <button className="nav-button" type="button">
      {icon}
      {label}
    </button>
  );
}

function Topbar({
  bootstrap,
  sessionTitle,
  workspacePath,
  inspectorOpen,
  isProcessing,
  onToggleInspector,
  terminalOpen,
  onToggleTerminal,
  currentProvider,
  currentModel,
  onProviderChange,
  onModelChange
}: {
  bootstrap: Bootstrap;
  sessionTitle: string;
  workspacePath: string | null;
  inspectorOpen: boolean;
  isProcessing: boolean;
  onToggleInspector: () => void;
  terminalOpen: boolean;
  onToggleTerminal: () => void;
  currentProvider: string;
  currentModel: string;
  onProviderChange: (provider: string) => void;
  onModelChange: (model: string) => void;
}) {
  const providerOptions = useMemo(() => {
    const saved = localStorage.getItem("yode-llm-providers");
    let list: any[] = [];
    if (saved) {
      try {
        const data = JSON.parse(saved);
        if (Array.isArray(data)) {
          list = data;
        } else if (data && typeof data === "object") {
          list = Object.values(data);
        }
      } catch (e) {}
    }
    const enabledProviders = list.filter((p: any) => p && p.enabled);
    if (enabledProviders.length === 0) {
      return PROVIDERS_META.map((p) => ({
        value: p.id,
        label: p.nameEn
      }));
    }
    return enabledProviders.map((p: any) => ({
      value: p.id,
      label: p.name || p.id
    }));
  }, []);

  return (
    <header className="topbar" data-tauri-drag-region>
      <div className="title-stack" data-tauri-drag-region>
        <div className="session-heading" data-tauri-drag-region>{sessionTitle}</div>
        {workspacePath && (
          <div className="workspace-path" data-tauri-drag-region>
            <span data-tauri-drag-region>{workspacePath}</span>
            <span>main</span>
          </div>
        )}
      </div>
      <div className="runtime-strip" aria-label="运行状态" style={{ display: "flex", gap: "8px", alignItems: "center" }}>
        <DropdownPill
          icon={<TopbarProviderIcon id={currentProvider} />}
          label={getProviderName(currentProvider)}
          value={currentProvider}
          options={providerOptions}
          onChange={onProviderChange}
        />
        <button className="icon-button" type="button" data-tauri-no-drag title="更多">
          <MoreHorizontal size={18} />
        </button>
        <button
          className={`icon-button ${terminalOpen ? "active" : ""}`}
          onClick={onToggleTerminal}
          data-tauri-no-drag
          type="button"
          title={terminalOpen ? "收起终端" : "打开终端"}
        >
          <TerminalSquare size={18} />
        </button>
        <button
          className="icon-button"
          onClick={onToggleInspector}
          data-tauri-no-drag
          type="button"
          title={inspectorOpen ? "收起运行详情" : "展开运行详情"}
        >
          {inspectorOpen ? <PanelRightClose size={18} /> : <PanelRight size={18} />}
        </button>
      </div>
    </header>
  );
}

function TopbarProviderIcon({ id }: { id: string }) {
  const [failed, setFailed] = useState(false);
  if (failed) {
    return <span style={{ width: "14px", height: "14px", display: "inline-block" }} />;
  }
  const aliases: Record<string, string> = {
    baidu: "baidu-qianfan",
    ali: "dashscope-coding",
    qwen: "qwen",
    google: "gemini"
  };
  const iconId = aliases[id] || id;
  const src = `/provider-icons/${iconId}.png`;
  return (
    <img
      src={src}
      alt=""
      style={{ width: "14px", height: "14px", objectFit: "contain", borderRadius: "2px", display: "block" }}
      onError={() => setFailed(true)}
    />
  );
}

function getProviderName(providerId: string) {
  const saved = localStorage.getItem("yode-llm-providers");
  if (saved) {
    try {
      const data = JSON.parse(saved);
      const list = Array.isArray(data) ? data : Object.values(data);
      const found = list.find((p: any) => p.id === providerId);
      if (found && found.name) {
        return found.name;
      }
    } catch (e) {}
  }
  const preset = PROVIDERS_META.find(p => p.id === providerId);
  return preset?.name || providerId;
}

function DropdownPill({
  icon,
  label,
  options,
  value,
  onChange,
  disabled
}: {
  icon: React.ReactNode;
  label: string;
  options: { value: string; label: string }[];
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  return (
    <div ref={ref} style={{ position: "relative" }}>
      <button
        type="button"
        data-tauri-no-drag
        disabled={disabled}
        onClick={() => setIsOpen(!isOpen)}
        className="status-pill quiet"
        style={{
          cursor: disabled ? "default" : "pointer",
          display: "flex",
          alignItems: "center",
          gap: "6px",
          border: "none",
          background: "var(--field)",
          padding: "4px 8px",
          borderRadius: "var(--radius)",
          color: "var(--text-soft)",
          fontSize: "12px",
          transition: "background 150ms, color 150ms"
        }}
        onMouseEnter={(e) => {
          if (!disabled) {
            e.currentTarget.style.background = "color-mix(in oklch, var(--accent-muted), transparent 60%)";
            e.currentTarget.style.color = "var(--text)";
          }
        }}
        onMouseLeave={(e) => {
          if (!disabled) {
            e.currentTarget.style.background = "var(--field)";
            e.currentTarget.style.color = "var(--text-soft)";
          }
        }}
      >
        {icon}
        <span>{label}</span>
        {!disabled && <ChevronDown size={11} style={{ opacity: 0.7, transform: isOpen ? "rotate(180deg)" : "none", transition: "transform 150ms" }} />}
      </button>

      {isOpen && (
        <div
          className="context-dropdown"
          style={{
            position: "absolute",
            top: "calc(100% + 6px)",
            bottom: "auto",
            left: 0,
            width: "200px"
          }}
        >
          {options.map((opt) => {
            const isSelected = opt.value === value;
            return (
              <button
                key={opt.value}
                type="button"
                data-tauri-no-drag
                className={`context-option ${isSelected ? "selected" : ""}`}
                onClick={() => {
                  onChange(opt.value);
                  setIsOpen(false);
                }}
              >
                <TopbarProviderIcon id={opt.value} />
                <span>{opt.label}</span>
                {isSelected ? <Check size={14} style={{ color: "var(--accent)" }} /> : <span />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

function StatusPill({
  icon,
  label,
  tone
}: {
  icon: React.ReactNode;
  label: string;
  tone?: "live" | "quiet";
}) {
  return (
    <span className={`status-pill ${tone ?? ""}`}>
      {icon}
      {label}
    </span>
  );
}

function ChatWorkspace({
  draft,
  timelineItems,
  onDraftChange,
  onSendMessage,
  inspectorOpen,
  isProcessing,
  onCancelMessage,
  permissionMode,
  onPermissionModeChange,
  onPermissionResolved,
  appLang,
  projectOptions,
  selectedProjectRoot,
  onProjectRootChange,
  onAddProject,
  currentProvider,
  currentModel,
  onModelChange
}: {
  draft: string;
  timelineItems: TimelineItem[];
  onDraftChange: (value: string) => void;
  onSendMessage: () => void;
  inspectorOpen: boolean;
  isProcessing: boolean;
  onCancelMessage: () => void;
  permissionMode: string;
  onPermissionModeChange: (mode: string) => void;
  onPermissionResolved: (id: string) => void;
  appLang: string;
  projectOptions: Array<{ label: string; root: string | null }>;
  selectedProjectRoot: string | null;
  onProjectRootChange: (root: string | null) => void;
  onAddProject: () => Promise<void>;
  currentProvider: string;
  currentModel: string;
  onModelChange: (model: string) => void;
}) {
  // Check if assistant is currently streaming (has any running status or last item kind is not fully completed)
  const isStreaming = useMemo(() => {
    // If there is any item with status === 'running', it is still streaming/running.
    const hasRunningTool = timelineItems.some(item => item.kind === "tool" && item.status === "running");
    if (hasRunningTool) return true;
    
    // Fallback: if last item is reasoning or assistant without stream complete metadata, consider it active
    const lastItem = timelineItems[timelineItems.length - 1];
    if (!lastItem) return false;
    if (lastItem.kind === "assistant" && lastItem.meta !== "stream complete") {
      return true;
    }
    return false;
  }, [timelineItems]);

  // Track expanded turn IDs separately
  const [expandedTurnIds, setExpandedTurnIds] = useState<string[]>([]);

  const turns = useMemo(() => {
    const list: Array<{
      id: string;
      userItem: TimelineItem | null;
      items: TimelineItem[];
      hasIntermediate: boolean;
    }> = [];

    let currentTurn: typeof list[number] = {
      id: "welcome",
      userItem: null,
      items: [],
      hasIntermediate: false
    };

    timelineItems.forEach((item) => {
      if (item.kind === "user") {
        if (currentTurn.userItem || currentTurn.items.length > 0) {
          list.push(currentTurn);
        }
        currentTurn = {
          id: item.id,
          userItem: item,
          items: [],
          hasIntermediate: false
        };
      } else {
        currentTurn.items.push(item);
        if (item.kind === "tool" || item.kind === "reasoning" || (item.kind === "assistant" && isIntermediateAssistantItem(item))) {
          currentTurn.hasIntermediate = true;
        }
      }
    });

    if (currentTurn.userItem || currentTurn.items.length > 0) {
      list.push(currentTurn);
    }

    return list;
  }, [timelineItems]);

  const activePermission = [...timelineItems]
    .reverse()
    .find((item): item is Extract<TimelineItem, { kind: "permission" }> => item.kind === "permission");
  const timelinePanelRef = useRef<HTMLElement | null>(null);
  const shouldStickToBottomRef = useRef(true);
  const lastTimelineLengthRef = useRef(0);

  const scrollTimelineToBottom = (behavior: ScrollBehavior = "smooth") => {
    const panel = timelinePanelRef.current;
    if (!panel) return;
    panel.scrollTo({
      top: panel.scrollHeight,
      behavior
    });
  };

  const handleTimelineScroll = () => {
    const panel = timelinePanelRef.current;
    if (!panel) return;
    const distanceToBottom = panel.scrollHeight - panel.scrollTop - panel.clientHeight;
    shouldStickToBottomRef.current = distanceToBottom < 120;
  };

  useLayoutEffect(() => {
    if (!shouldStickToBottomRef.current) return;
    const itemAdded = timelineItems.length > lastTimelineLengthRef.current;
    lastTimelineLengthRef.current = timelineItems.length;
    const frame = window.requestAnimationFrame(() => {
      scrollTimelineToBottom(itemAdded && !isStreaming ? "smooth" : "auto");
    });
    return () => window.cancelAnimationFrame(frame);
  }, [timelineItems.length, isStreaming]);

  return (
    <div className={`chat-layout ${inspectorOpen ? "" : "inspector-collapsed"}`}>
      <div className="conversation-column">
        <section
          className="timeline-panel"
          aria-label="会话时间线"
          ref={timelinePanelRef}
          onScroll={handleTimelineScroll}
        >
          <div className="timeline-header">
            <span>RUN LOG</span>
            <strong>desktop-runtime</strong>
            <em>{timelineItems.length} events</em>
          </div>
          
          {turns.map((turn, turnIndex) => {
            const isExpanded = expandedTurnIds.includes(turn.id);
            const isLastTurn = turnIndex === turns.length - 1;

            const visibleItems = turn.items.filter((item) => {
              if (item.kind === "permission") return false;
              if (item.kind === "boundary") return false;

              const isIntermediate = item.kind === "tool" || item.kind === "reasoning" || (item.kind === "assistant" && isIntermediateAssistantItem(item));
              if (isIntermediate) {
                return isExpanded;
              }

              if (item.kind === "assistant" && !isIntermediateAssistantItem(item)) {
                if (isProcessing && isLastTurn) {
                  return false;
                }
              }

              return true;
            });

            const foldableCount = turn.items.filter(
              (item) => item.kind === "tool" || item.kind === "reasoning" || (item.kind === "assistant" && isIntermediateAssistantItem(item))
            ).length;

            const isFoldable = foldableCount > 0;
            const toggleText = isExpanded
              ? `已展开 ${foldableCount} 个思考与执行步骤`
              : (isProcessing && isLastTurn)
                ? `正在处理，已折叠 ${foldableCount} 个思考与执行步骤...`
                : `已省略 ${foldableCount} 个中间思考与执行步骤...`;
            const actionText = isExpanded ? "收起详情" : "展开详情";

            return (
              <React.Fragment key={turn.id}>
                {turn.userItem && <TimelineNode item={turn.userItem} appLang={appLang} />}
                
                {isFoldable && (
                  <div 
                    onClick={() => {
                      if (isExpanded) {
                        setExpandedTurnIds(prev => prev.filter(id => id !== turn.id));
                      } else {
                        setExpandedTurnIds(prev => [...prev, turn.id]);
                      }
                    }}
                    style={{
                      margin: "8px auto 14px",
                      maxWidth: "760px",
                      background: "var(--field)",
                      border: "1px dashed var(--line-soft)",
                      borderRadius: "var(--radius)",
                      padding: "8px 12px",
                      cursor: "pointer",
                      fontSize: "11.5px",
                      color: "var(--text-soft)",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      transition: "all 0.15s ease",
                    }}
                    onMouseEnter={(e) => {
                      e.currentTarget.style.borderColor = "var(--accent)";
                      e.currentTarget.style.color = "var(--text)";
                    }}
                    onMouseLeave={(e) => {
                      e.currentTarget.style.borderColor = "var(--line-soft)";
                      e.currentTarget.style.color = "var(--text-soft)";
                    }}
                  >
                    <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                      <span style={{
                        width: "6px",
                        height: "6px",
                        borderRadius: "50%",
                        background: "var(--accent)"
                      }} />
                      <span>{toggleText}</span>
                    </div>
                    <span style={{ fontWeight: "600", fontSize: "11px", color: "var(--accent)" }}>{actionText}</span>
                  </div>
                )}

                {visibleItems.map((item) => (
                  <TimelineNode key={item.id} item={item} appLang={appLang} />
                ))}
              </React.Fragment>
            );
          })}
        </section>
        {activePermission ? (
          <div className="permission-dock" aria-label="执行确认">
            <PermissionActions
              item={activePermission}
              appLang={appLang}
              onResolved={() => onPermissionResolved(activePermission.id)}
            />
          </div>
        ) : null}
        <Composer
          draft={draft}
          onDraftChange={onDraftChange}
          onSendMessage={onSendMessage}
          isProcessing={isProcessing}
          onCancelMessage={onCancelMessage}
          permissionMode={permissionMode}
          onPermissionModeChange={onPermissionModeChange}
          appLang={appLang}
          projectOptions={projectOptions}
          selectedProjectRoot={selectedProjectRoot}
          onProjectRootChange={onProjectRootChange}
          onAddProject={onAddProject}
          currentProvider={currentProvider}
          currentModel={currentModel}
          onModelChange={onModelChange}
        />
      </div>
      <RunInspector
        isProcessing={isProcessing}
        permissionMode={permissionMode}
        timelineItems={timelineItems}
      />
    </div>
  );
}

function TimelineNode({ item, appLang }: { item: TimelineItem; appLang: string }) {
  if (item.kind === "boundary") {
    return (
      <div className="boundary-node">
        <span>{item.title}</span>
        <p>{item.body}</p>
      </div>
    );
  }

  if (item.kind === "user") {
    return (
      <div 
        className="timeline-node user-bubble-container" 
        style={{ 
          display: "flex", 
          justifyContent: "flex-end", 
          width: "100%", 
          maxWidth: "760px",
          margin: "0 auto 12px",
          paddingLeft: "24px"
        }}
      >
        <div 
          className="user-chat-bubble"
          title={item.body}
          style={{
            background: "color-mix(in oklch, var(--accent), transparent 85%)",
            border: "none",
            borderRadius: "14px 14px 2px 14px",
            padding: "10px 14px",
            maxWidth: "85%",
            boxShadow: "0 2px 8px rgba(0, 0, 0, 0.15)",
            display: "block",
            overflow: "hidden"
          }}
        >
          <p style={{ 
            margin: 0, 
            color: "var(--text)", 
            fontSize: "13px", 
            lineHeight: "1.45", 
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis"
          }}>
            {item.body}
          </p>
        </div>
      </div>
    );
  }

  const icon =
    item.kind === "tool" ? (
      <Hammer size={18} />
    ) : item.kind === "permission" ? (
      <ShieldCheck size={18} />
    ) : item.kind === "reasoning" ? (
      <TerminalSquare size={18} />
    ) : (
      <Bot size={18} />
    );

  return (
    <article className={`timeline-node ${item.kind}`}>
      <div className="node-rail">
        <div className="node-icon">{icon}</div>
      </div>
      <div className="node-content">
        <div className="node-header">
          <h2>{item.title}</h2>
          {"meta" in item && item.meta ? <span>{item.meta}</span> : null}
        </div>
        {item.kind === "assistant" ? (
          <MarkdownContent text={item.body} />
        ) : (
          <p>{item.body}</p>
        )}
        {item.kind === "tool" ? <ToolMeta item={item} /> : null}
        {item.kind === "permission" ? <PermissionActions item={item} appLang={appLang} /> : null}
      </div>
    </article>
  );
}

function isIntermediateAssistantItem(item: TimelineItem) {
  if (item.kind !== "assistant") return false;
  const body = item.body.trim();
  return item.meta === "intermediate" || body === "" || body === "." || body === "..." || body === "…";
}

function MarkdownContent({ text }: { text: string }) {
  const blocks = useMemo(() => parseMarkdownBlocks(text), [text]);
  return (
    <div className="markdown-content">
      {blocks.map((block, index) => {
        if (block.type === "heading") {
          const Tag = `h${Math.min(block.level, 4)}` as keyof JSX.IntrinsicElements;
          return <Tag key={index}>{renderInlineMarkdown(block.text)}</Tag>;
        }
        if (block.type === "code") {
          return <pre key={index}><code>{block.text}</code></pre>;
        }
        if (block.type === "list") {
          return (
            <ul key={index}>
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
              ))}
            </ul>
          );
        }
        if (block.type === "table") {
          return (
            <div key={index} className="markdown-table-wrapper" style={{ overflowX: "auto", margin: "12px 0" }}>
              <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "12px" }}>
                <thead>
                  <tr style={{ borderBottom: "2px solid var(--line)" }}>
                    {block.headers.map((h, i) => (
                      <th key={i} style={{ padding: "8px", textAlign: "left", fontWeight: "bold" }}>
                        {renderInlineMarkdown(h)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {block.rows.map((row, ri) => (
                    <tr key={ri} style={{ borderBottom: "1px solid var(--line-soft)" }}>
                      {row.map((cell, ci) => (
                        <td key={ci} style={{ padding: "8px" }}>
                          {renderInlineMarkdown(cell)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          );
        }
        if (block.type === "divider") {
          return <hr key={index} style={{ border: "0", borderTop: "1px solid var(--line-soft)", margin: "16px 0" }} />;
        }
        return <p key={index}>{renderInlineMarkdown(block.text)}</p>;
      })}
    </div>
  );
}

type MarkdownBlock =
  | { type: "heading"; level: number; text: string }
  | { type: "code"; text: string }
  | { type: "list"; items: string[] }
  | { type: "table"; headers: string[]; rows: string[][] }
  | { type: "divider" }
  | { type: "paragraph"; text: string };

function parseMarkdownBlocks(text: string): MarkdownBlock[] {
  const blocks: MarkdownBlock[] = [];
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  let paragraph: string[] = [];
  let list: string[] = [];
  let tableRows: string[][] = [];
  let code: string[] | null = null;

  const flushParagraph = () => {
    if (paragraph.length > 0) {
      blocks.push({ type: "paragraph", text: paragraph.join(" ") });
      paragraph = [];
    }
  };
  const flushList = () => {
    if (list.length > 0) {
      blocks.push({ type: "list", items: list });
      list = [];
    }
  };
  const flushTable = () => {
    if (tableRows.length > 0) {
      if (tableRows.length >= 2 && tableRows[1].every(cell => /^:?-+:?$/.test(cell.trim()))) {
        const headers = tableRows[0];
        const rows = tableRows.slice(2);
        blocks.push({ type: "table", headers, rows });
      } else {
        for (const row of tableRows) {
          paragraph.push("|" + row.join("|") + "|");
        }
      }
      tableRows = [];
    }
  };

  for (const line of lines) {
    if (line.trim().startsWith("```")) {
      if (code) {
        blocks.push({ type: "code", text: code.join("\n") });
        code = null;
      } else {
        flushParagraph();
        flushList();
        flushTable();
        code = [];
      }
      continue;
    }

    if (code) {
      code.push(line);
      continue;
    }

    const heading = line.match(/^(#{1,4})\s+(.+)$/);
    if (heading) {
      flushParagraph();
      flushList();
      flushTable();
      blocks.push({ type: "heading", level: heading[1].length, text: heading[2].trim() });
      continue;
    }

    const listItem = line.match(/^\s*[-*]\s+(.+)$/);
    if (listItem) {
      flushParagraph();
      flushTable();
      list.push(listItem[1].trim());
      continue;
    }

    const isDivider = /^(?:-{3,}|\*{3,}|_{3,})$/.test(line.trim());
    if (isDivider) {
      flushParagraph();
      flushList();
      flushTable();
      blocks.push({ type: "divider" });
      continue;
    }

    const isTableRow = line.trim().startsWith("|") && line.trim().endsWith("|");
    if (isTableRow) {
      flushParagraph();
      flushList();
      const cells = line.split("|").map(c => c.trim()).slice(1, -1);
      tableRows.push(cells);
      continue;
    }

    if (!line.trim()) {
      flushParagraph();
      flushList();
      flushTable();
      continue;
    }

    flushList();
    flushTable();
    paragraph.push(line.trim());
  }

  if (code) blocks.push({ type: "code", text: code.join("\n") });
  flushParagraph();
  flushList();
  flushTable();
  return blocks.length > 0 ? blocks : [{ type: "paragraph", text }];
}

function renderInlineMarkdown(text: string) {
  const parts = text.split(/(`[^`]+`|\*\*[^*]+\*\*)/g).filter(Boolean);
  return parts.map((part, index) => {
    if (part.startsWith("`") && part.endsWith("`")) {
      return <code key={index}>{part.slice(1, -1)}</code>;
    }
    if (part.startsWith("**") && part.endsWith("**")) {
      return <strong key={index}>{part.slice(2, -2)}</strong>;
    }
    return <React.Fragment key={index}>{part}</React.Fragment>;
  });
}

function ToolMeta({ item }: { item: Extract<TimelineItem, { kind: "tool" }> }) {
  return (
    <div className="tool-meta">
      <span className={`tool-state ${item.status}`}>{statusLabel(item.status)}</span>
      <code>{item.tool}</code>
      <button className="ghost-button" type="button">
        open
      </button>
    </div>
  );
}

function PermissionActions({
  item,
  appLang,
  onResolved
}: {
  item: Extract<TimelineItem, { kind: "permission" }>;
  appLang: string;
  onResolved?: () => void;
}) {
  const isZh = appLang === "zh";

  const options = [
    {
      id: "allow_once",
      label: isZh ? "允许本次执行" : "Yes, allow this time",
      description: isZh ? "仅允许本次执行" : "Only allow this execution"
    },
    {
      id: "always_allow",
      label: isZh ? "总是允许此命令" : "Yes, always allow this command",
      description: isZh ? "后续同类命令不再询问" : "Do not ask again for similar commands"
    },
    {
      id: "deny",
      label: isZh ? "拒绝并改用其他方式" : "No",
      description: isZh ? "告诉 agent 改用其他方式" : "Tell agent to use another way"
    }
  ] as const;

  const [selectedIndex, setSelectedIndex] = useState(0);
  const selectedOption = options[selectedIndex];
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const respond = (decision: (typeof options)[number]["id"]) => {
    onResolved?.();
    if (item.sessionId && item.turnId) {
      invoke("permission_respond", {
        sessionId: item.sessionId,
        turnId: item.turnId,
        allow: decision !== "deny",
        alwaysAllow: decision === "always_allow"
      }).catch(console.error);
    }
  };

  useEffect(() => {
    optionRefs.current[selectedIndex]?.focus();
  }, [selectedIndex]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((index) => (index - 1 + options.length) % options.length);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((index) => (index + 1) % options.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        respond(selectedOption.id);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedOption.id, item.sessionId, item.turnId]);

  return (
    <div className="permission-prompt">
      <div className="permission-prompt-title">
        <TerminalSquare size={16} />
        <span>{isZh ? "允许运行此命令吗？" : "Allow running this command?"}</span>
      </div>
      <pre className="permission-command">{item.body || item.tool}</pre>
      <div className="permission-option-list">
        {options.map((option, index) => (
          <button
            className={`permission-option ${selectedIndex === index ? "selected" : ""}`}
            key={option.id}
            ref={(node) => {
              optionRefs.current[index] = node;
            }}
            onClick={() => {
              setSelectedIndex(index);
              respond(option.id);
            }}
            type="button"
            style={{ outline: "none", boxShadow: "none" }}
          >
            <kbd>{index + 1}</kbd>
            <span>{option.label}</span>
            <em>{option.description}</em>
          </button>
        ))}
      </div>
      <div className="permission-prompt-footer">
        <button className="permission-skip" onClick={() => respond("deny")} type="button" style={{ outline: "none", boxShadow: "none" }}>
          {isZh ? "跳过" : "Skip"}
        </button>
        <button className="permission-submit" onClick={() => respond(selectedOption.id)} type="button" style={{ outline: "none", boxShadow: "none" }}>
          {isZh ? "提交" : "Submit"}
          <span>↵</span>
        </button>
      </div>
    </div>
  );
}

function statusLabel(status: "running" | "success" | "blocked") {
  if (status === "running") return "运行中";
  if (status === "success") return "完成";
  return "阻塞";
}

function messagesToTimelineItems(messages: DesktopMessage[]): TimelineItem[] {
  return messages.flatMap((message): TimelineItem[] => {
    const content = message.content?.trim();
    const reasoning = message.reasoning?.trim();
    const timestamp = formatHistoryTimestamp(message.createdAt);

    if (message.role === "user") {
      return content
        ? [{
            id: `history-${message.id}`,
            kind: "user",
            title: "用户",
            body: content,
            meta: timestamp
          }]
        : [];
    }

    if (message.role === "assistant") {
      const items: TimelineItem[] = [];
      if (reasoning) {
        items.push({
          id: `history-${message.id}-reasoning`,
          kind: "reasoning",
          title: "思考",
          body: reasoning,
          meta: "complete"
        });
      }
      if (content) {
        items.push({
          id: `history-${message.id}`,
          kind: "assistant",
          title: "Yode",
          body: content,
          meta: "stream complete"
        });
      }
      parseToolCalls(message.toolCallsJson).forEach((toolCall, index) => {
        items.push({
          id: `history-${message.id}-tool-call-${index}`,
          kind: "tool",
          title: `调用工具: ${toolCall.name}`,
          body: toolCall.arguments,
          tool: toolCall.name,
          status: "success",
          meta: "history"
        });
      });
      return items;
    }

    if (message.role === "tool") {
      return [{
        id: `history-${message.id}`,
        kind: "tool",
        title: "工具结果",
        body: content || message.toolCallId || "",
        tool: message.toolCallId || "tool",
        status: "success"
      }];
    }

    return [];
  });
}

function parseToolCalls(raw: string | null | undefined): Array<{ name: string; arguments: string }> {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.flatMap((item) => {
      const name = stringValue(item?.name) ?? stringValue(item?.function?.name);
      const args =
        stringValue(item?.arguments) ??
        stringValue(item?.function?.arguments) ??
        JSON.stringify(item?.arguments ?? item?.function?.arguments ?? {});
      return name ? [{ name, arguments: args }] : [];
    });
  } catch {
    return [];
  }
}

function formatHistoryTimestamp(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return undefined;
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  });
}

function projectLabelFromPath(path: string) {
  const trimmed = path.trim();
  if (!trimmed) return "项目";
  const parts = trimmed.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] || trimmed;
}

function deriveSessionTitle(content: string) {
  const normalized = content.replace(/\s+/g, " ").trim();
  if (!normalized) return "新对话";
  return normalized.length > 28 ? normalized.slice(0, 28) : normalized;
}

function upsertActiveSession(items: SessionSummary[], session: SessionSummary) {
  const nextSession = { ...session, active: true };
  const exists = items.some((item) => item.id === session.id);
  if (!exists) {
    return [
      nextSession,
      ...items.map((item) => item.active ? { ...item, active: false } : item)
    ];
  }
  return items.map((item) =>
    item.id === session.id
      ? nextSession
      : item.active
        ? { ...item, active: false }
        : item
  );
}

function Composer({
  draft,
  onDraftChange,
  onSendMessage,
  isProcessing,
  onCancelMessage,
  permissionMode,
  onPermissionModeChange,
  appLang,
  projectOptions,
  selectedProjectRoot,
  onProjectRootChange,
  onAddProject,
  currentProvider,
  currentModel,
  onModelChange
}: {
  draft: string;
  onDraftChange: (value: string) => void;
  onSendMessage: () => void;
  isProcessing: boolean;
  onCancelMessage: () => void;
  permissionMode: string;
  onPermissionModeChange: (mode: string) => void;
  appLang: string;
  projectOptions: Array<{ label: string; root: string | null }>;
  selectedProjectRoot: string | null;
  onProjectRootChange: (root: string | null) => void;
  onAddProject: () => Promise<void>;
  currentProvider: string;
  currentModel: string;
  onModelChange: (model: string) => void;
}) {
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [projectDropdownOpen, setProjectDropdownOpen] = useState(false);
  const [modelDropdownOpen, setModelDropdownOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const projectDropdownRef = useRef<HTMLDivElement>(null);
  const modelDropdownRef = useRef<HTMLDivElement>(null);

  const isZh = appLang === "zh";

  const modelOptions = useMemo(() => {
    const saved = localStorage.getItem("yode-llm-providers");
    let list: any[] = [];
    if (saved) {
      try {
        const data = JSON.parse(saved);
        if (Array.isArray(data)) {
          list = data;
        } else if (data && typeof data === "object") {
          list = Object.values(data);
        }
      } catch (e) {}
    }
    const found = list.find((p: any) => p && p.id === currentProvider);
    if (found && Array.isArray(found.models) && found.models.length > 0) {
      return found.models;
    }
    const meta = PROVIDERS_META.find((p) => p.id === currentProvider);
    return meta ? meta.defaultModels : [];
  }, [currentProvider]);

  const OPTIONS = [
    {
      key: "default",
      label: isZh ? "每次询问" : "Ask for approval",
      description: isZh ? "修改外部文件及使用网络时，总是需要确认" : "Always ask to edit external files and use the internet",
      icon: <Hand size={15} />
    },
    {
      key: "auto",
      label: isZh ? "自动授权安全操作" : "Approve for me",
      description: isZh ? "仅对检测到存在潜在风险的操作进行询问" : "Only ask for actions detected as potentially unsafe",
      icon: <Shield size={15} />
    },
    {
      key: "bypass",
      label: isZh ? "完全信任" : "Full access",
      description: isZh ? "不受限制地访问网络及您计算机上的任何文件" : "Unrestricted access to the internet and any file on your computer",
      icon: <AlertCircle size={15} />
    }
  ];

  const currentOption = OPTIONS.find(
    (o) => o.key.toLowerCase() === (permissionMode || "default").toLowerCase()
  ) || OPTIONS[0];
  const currentProject =
    selectedProjectRoot === null
      ? projectOptions.find((option) => option.root === null) ?? {
          label: isZh ? "独立对话" : "Standalone",
          root: null
        }
      : projectOptions.find((option) => option.root === selectedProjectRoot) ??
        projectOptions[0] ?? {
          label: isZh ? "当前项目" : "Current project",
          root: selectedProjectRoot ?? null
        };

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setDropdownOpen(false);
      }
      if (
        projectDropdownRef.current &&
        !projectDropdownRef.current.contains(event.target as Node)
      ) {
        setProjectDropdownOpen(false);
      }
      if (
        modelDropdownRef.current &&
        !modelDropdownRef.current.contains(event.target as Node)
      ) {
        setModelDropdownOpen(false);
      }
    }
    if (dropdownOpen || projectDropdownOpen || modelDropdownOpen) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [dropdownOpen, projectDropdownOpen, modelDropdownOpen]);

  return (
    <footer className="composer" style={{ position: "relative" }}>
      <textarea
        aria-label="消息"
        placeholder={isZh ? "输入仓库任务..." : "Enter repository task..."}
        value={draft}
        onChange={(event) => onDraftChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey) {
            if (event.metaKey || event.ctrlKey) {
              // Cmd+Enter or Ctrl+Enter -> Newline
              event.preventDefault();
              const target = event.target as HTMLTextAreaElement;
              const start = target.selectionStart;
              const end = target.selectionEnd;
              const val = target.value;
              const nextVal = val.substring(0, start) + "\n" + val.substring(end);
              onDraftChange(nextVal);
              // reset cursor position
              setTimeout(() => {
                target.selectionStart = target.selectionEnd = start + 1;
              }, 0);
            } else {
              // Plain Enter -> Send / Queue
              event.preventDefault();
              onSendMessage();
            }
          }
        }}
      />
      <div className="composer-toolbar">
        <div className="composer-tools" style={{ position: "relative" }}>
          <button className="icon-button" type="button" title={isZh ? "附件" : "Attachment"} style={{ outline: "none", boxShadow: "none" }}>
            <Paperclip size={17} />
          </button>

          <div ref={projectDropdownRef} style={{ display: "inline-block", position: "relative" }}>
            <button
              className="mode-chip"
              type="button"
              onClick={() => setProjectDropdownOpen(!projectDropdownOpen)}
              title={currentProject.root ?? (isZh ? "独立对话" : "Standalone")}
              style={{ outline: "none", boxShadow: "none", cursor: "pointer" }}
            >
              <Folder size={15} />
              {currentProject.label}
            </button>

            {projectDropdownOpen && (
              <div className="context-dropdown project-dropdown">
                {projectOptions.map((option) => {
                  const selected = option.root === selectedProjectRoot;
                  return (
                    <button
                      key={option.root ?? "__standalone__"}
                      type="button"
                      className={`context-option ${selected ? "selected" : ""}`}
                      onClick={() => {
                        onProjectRootChange(option.root);
                        setProjectDropdownOpen(false);
                      }}
                    >
                      <Folder size={14} />
                      <span>{option.label}</span>
                      {selected ? <Check size={14} /> : null}
                    </button>
                  );
                })}
                <div className="context-dropdown-divider" />
                <button
                  type="button"
                  className="context-option context-option-action"
                  onClick={() => {
                    setProjectDropdownOpen(false);
                    void onAddProject();
                  }}
                >
                  <FolderPlus size={14} />
                  <span>{isZh ? "添加项目..." : "Add project..."}</span>
                </button>
              </div>
            )}
          </div>
          
          <div ref={dropdownRef} style={{ display: "inline-block" }}>
            <button
              className="mode-chip"
              type="button"
              onClick={() => setDropdownOpen(!dropdownOpen)}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: "6px",
                cursor: "pointer",
                position: "relative",
                outline: "none",
                boxShadow: "none"
              }}
            >
              {currentOption.icon}
              {currentOption.label}
            </button>

            {dropdownOpen && (
              <div
                className="permission-dropdown"
                style={{
                  position: "absolute",
                  bottom: "100%",
                  left: "0",
                  marginBottom: "8px",
                  zIndex: 1000,
                  width: "380px",
                  background: "var(--panel)",
                  border: "1px solid var(--line)",
                  borderRadius: "8px",
                  boxShadow: "0 4px 20px rgba(0, 0, 0, 0.3)",
                  padding: "16px",
                  display: "flex",
                  flexDirection: "column",
                  gap: "12px"
                }}
              >
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center"
                  }}
                >
                  <span
                    style={{
                      fontSize: "12px",
                      color: "var(--text-soft)",
                      fontWeight: 500
                    }}
                  >
                    {isZh ? "如何授权 Yode 的操作？" : "How should Yode actions be approved?"}
                  </span>
                  <a
                    href="#"
                    onClick={(e) => e.preventDefault()}
                    style={{
                      fontSize: "12px",
                      color: "var(--text-soft)",
                      textDecoration: "underline"
                    }}
                  >
                    {isZh ? "了解更多" : "Learn more"}
                  </a>
                </div>

                <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                  {OPTIONS.map((option) => {
                    const isSelected = option.key.toLowerCase() === currentOption.key.toLowerCase();
                    return (
                      <button
                        key={option.key}
                        type="button"
                        onClick={() => {
                          onPermissionModeChange(option.key);
                          setDropdownOpen(false);
                        }}
                        style={{
                          display: "flex",
                          alignItems: "flex-start",
                          gap: "12px",
                          width: "100%",
                          padding: "10px",
                          background: isSelected ? "rgba(255, 255, 255, 0.05)" : "transparent",
                          border: "none",
                          borderRadius: "6px",
                          textAlign: "left",
                          cursor: "pointer",
                          transition: "background 0.2s",
                          outline: "none",
                          boxShadow: "none"
                        }}
                        className="dropdown-option-btn"
                      >
                        <div style={{ marginTop: "2px", color: isSelected ? "var(--accent)" : "var(--text-soft)" }}>
                          {option.icon}
                        </div>
                        <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: "2px" }}>
                          <span style={{ fontSize: "13px", fontWeight: 500, color: "var(--text)" }}>
                            {option.label}
                          </span>
                          <span style={{ fontSize: "11px", color: "var(--text-soft)", lineHeight: "1.4" }}>
                            {option.description}
                          </span>
                        </div>
                        {isSelected && (
                          <Check size={14} style={{ color: "var(--accent)", alignSelf: "center" }} />
                        )}
                      </button>
                    );
                  })}
                </div>
              </div>
            )}
          </div>

          <div ref={modelDropdownRef} style={{ display: "inline-block", position: "relative" }}>
            <button
              className="mode-chip"
              type="button"
              onClick={() => setModelDropdownOpen(!modelDropdownOpen)}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: "6px",
                cursor: "pointer",
                outline: "none",
                boxShadow: "none"
              }}
            >
              <TopbarProviderIcon id={currentProvider} />
              <span>{currentModel || (isZh ? "选择模型" : "Select model")}</span>
              <ChevronDown size={11} style={{ opacity: 0.7, transform: modelDropdownOpen ? "rotate(180deg)" : "none", transition: "transform 150ms" }} />
            </button>

            {modelDropdownOpen && (
              <div className="context-dropdown model-dropdown">
                {modelOptions.map((model: string) => {
                  const selected = model === currentModel;
                  return (
                    <button
                      key={model}
                      type="button"
                      className={`context-option ${selected ? "selected" : ""}`}
                      onClick={() => {
                        onModelChange(model);
                        setModelDropdownOpen(false);
                      }}
                    >
                      <TopbarProviderIcon id={currentProvider} />
                      <span>{model}</span>
                      {selected ? <Check size={14} style={{ color: "var(--accent)" }} /> : <span />}
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </div>
        <div className="composer-actions">
          {isProcessing ? (
            <button className="send-button stop-button" onClick={onCancelMessage} type="button" title={isZh ? "终止" : "Stop"} style={{ background: "color-mix(in oklch, var(--error), transparent 30%)", borderColor: "color-mix(in oklch, var(--error), transparent 10%)", outline: "none", boxShadow: "none" }}>
              <Pause size={17} />
            </button>
          ) : (
            <button className="send-button" onClick={onSendMessage} type="button" title={isZh ? "发送" : "Send"} style={{ outline: "none", boxShadow: "none" }}>
              <Send size={17} />
            </button>
          )}
        </div>
      </div>
    </footer>
  );
}

function desktopEventToTimelineItem(
  payload: any,
  eventKind?: string
): TimelineItem {
  const outer = payload && typeof payload === "object" && "payload" in payload ? payload : null;
  const inner = outer ? outer.payload : payload;
  const sessionId = outer ? outer.sessionId : undefined;
  const turnId = outer ? outer.turnId : undefined;

  const kind = eventKind ?? stringValue(outer?.kind) ?? stringValue(inner?.kind) ?? stringValue(inner?.type);
  const eventId = stringValue(inner?.id);
  const tool = stringValue(inner?.tool) ?? "desktop";
  const title = stringValue(inner?.title) ?? "Yode";
  const body = stringValue(inner?.body) ?? "";
  const meta = stringValue(inner?.meta);
  const status = stringValue(inner?.status);

  if (kind === "turn_started") {
    return {
      id: turnId ? `reasoning-${turnId}` : `event-${Date.now()}-${Math.random()}`,
      kind: "reasoning",
      title: title || "思考中",
      body: body || "",
      meta: "running"
    };
  }

  if (kind === "permission" || kind === "tool_confirm_required" || kind === "plan_approval_required") {
    return {
      id: eventId ? `permission-${turnId}-${eventId}` : `event-${Date.now()}-${Math.random()}`,
      kind: "permission",
      title: title || "需要授权确认",
      body: body || `工具 "${tool}" 请求执行。`,
      tool: tool,
      risk: meta || "中等风险",
      sessionId,
      turnId
    };
  }

  if (kind === "ask_user") {
    return {
      id: eventId ? `ask-${turnId}-${eventId}` : `event-${Date.now()}-${Math.random()}`,
      kind: "assistant",
      title,
      body,
      meta: "waiting for input"
    };
  }

  if (kind === "tool_started" || kind === "tool_progress" || kind === "tool_result" || kind === "subagent_started" || kind === "subagent_completed" || inner?.tool) {
    return {
      id: eventId ? `tool-${turnId}-${eventId}` : `event-${Date.now()}-${Math.random()}`,
      kind: "tool",
      title,
      body,
      tool,
      status: status === "success" ? "success" : status === "blocked" ? "blocked" : "running",
      meta
    };
  }

  if (kind === "assistant_reasoning_delta") {
    return {
      id: `event-${Date.now()}-${Math.random()}`,
      kind: "reasoning",
      title,
      body,
      meta
    };
  }

  if (kind === "retrying") {
    return {
      id: "retrying-attempt",
      kind: "reasoning",
      title: `连接重试中 (${inner?.attempt}/${inner?.max_attempts})`,
      body: `倒计时 ${inner?.delay_secs} 秒后重试...\n\n${inner?.error_message || ""}`,
      meta: "running"
    };
  }

  if (
    kind === "usage_update" ||
    kind === "cost_update" ||
    kind === "context_compaction_started"
  ) {
    return {
      id: `event-${Date.now()}-${Math.random()}`,
      kind: "reasoning",
      title,
      body,
      meta: status === "running" ? "running" : meta
    };
  }

  if (
    kind === "context_compressed" ||
    kind === "done" ||
    kind === "plan_mode_entered" ||
    kind === "plan_mode_exited" ||
    kind === "session_memory_updated"
  ) {
    return {
      id: `event-${Date.now()}-${Math.random()}`,
      kind: "boundary",
      title,
      body
    };
  }

  return {
    id: `event-${Date.now()}-${Math.random()}`,
    kind: "assistant",
    title,
    body,
    meta
  };
}

function applyDesktopEventToTimelineItems(
  items: TimelineItem[],
  payload: any,
  eventKind?: string
): TimelineItem[] {
  const outer = payload && typeof payload === "object" && "payload" in payload ? payload : null;
  const inner = outer ? outer.payload : payload;
  const kind = eventKind ?? stringValue(outer?.kind) ?? stringValue(inner?.kind) ?? stringValue(inner?.type);
  const body = stringValue(inner?.body) ?? "";
  const reasoning = stringValue(inner?.reasoning) ?? "";
  const turnId = stringValue(outer?.turnId);
  const assistantId = turnId ? `assistant-${turnId}` : undefined;
  const reasoningId = turnId ? `reasoning-${turnId}` : undefined;
  const eventId = stringValue(inner?.id);
  const status = stringValue(inner?.status);
  const hasToolCalls = Boolean(inner?.hasToolCalls);

  if (kind === "tool_started" || kind === "tool_progress" || kind === "tool_result" || kind === "subagent_started" || kind === "subagent_completed") {
    const nextItem = desktopEventToTimelineItem(payload, eventKind);
    const existingIndex = items.findIndex((item) => item.id === nextItem.id);
    if (existingIndex >= 0 && nextItem.kind === "tool") {
      return items.map((item, index) =>
        index === existingIndex && item.kind === "tool"
          ? {
              ...item,
              title: nextItem.title || item.title,
              body: nextItem.body || item.body,
              status: nextItem.status,
              meta: nextItem.meta ?? item.meta
            }
          : item
      );
    }
    return [...items, nextItem];
  }

  if (kind === "turn_started") {
    const thinkingId = turnId ? `reasoning-${turnId}` : undefined;
    if (
      items.some((item) =>
        thinkingId
          ? item.id === thinkingId
          : item.kind === "reasoning" && item.meta === "running"
      )
    ) {
      return items;
    }
    return [...items, desktopEventToTimelineItem(payload, eventKind)];
  }

  if (kind === "assistant_text_delta") {
    const existingIndex = assistantId
      ? items.findIndex((item) => item.id === assistantId)
      : items.findIndex((item) => item.kind === "assistant" && item.meta !== "stream complete");
    if (existingIndex >= 0) {
      return items.map((item, index) =>
        index === existingIndex && item.kind === "assistant"
          ? { ...item, body: mergeStreamingText(item.body, body), meta: "streaming" }
          : item
      );
    }
    return [
      ...items,
      {
        id: assistantId ?? `event-${Date.now()}-${Math.random()}`,
        kind: "assistant",
        title: "Yode",
        body,
        meta: "streaming"
      }
    ];
  }

  if (kind === "assistant_text_complete") {
    const existingIndex = assistantId
      ? items.findIndex((item) => item.id === assistantId)
      : items.findIndex((item) => item.kind === "assistant" && item.meta !== "stream complete");
    if (existingIndex >= 0) {
      return items.map((item, index) =>
        index === existingIndex && item.kind === "assistant"
          ? { ...item, body: body || item.body, meta: "stream complete" }
          : item
      );
    }
    if (body) {
      return [
        ...items,
        {
          id: assistantId ?? `event-${Date.now()}-${Math.random()}`,
          kind: "assistant",
          title: "Yode",
          body,
          meta: "stream complete"
        }
      ];
    }
    return items;
  }

  if (kind === "assistant_reasoning_delta") {
    const existingIndex = reasoningId
      ? items.findIndex((item) => item.id === reasoningId)
      : items.findIndex((item) => item.kind === "reasoning" && item.meta === "running");
    if (existingIndex >= 0) {
      return items.map((item, index) =>
        index === existingIndex && item.kind === "reasoning"
          ? { ...item, body: mergeStreamingText(item.body, body) }
          : item
      );
    }
    return [
      ...items,
      {
        id: reasoningId ?? `event-${Date.now()}-${Math.random()}`,
        kind: "reasoning",
        title: "思考",
        body,
        meta: "running"
      }
    ];
  }

  if (kind === "retrying" || kind === "usage_update" || kind === "cost_update" || kind === "context_compaction_started") {
    const nextItem = desktopEventToTimelineItem(payload, eventKind);
    if (eventId || nextItem.id === "retrying-attempt") {
      const existingIndex = items.findIndex((item) => item.id === nextItem.id);
      if (existingIndex >= 0) {
        return items.map((item, index) => index === existingIndex ? nextItem : item);
      }
    }
    return [...items, nextItem];
  }

  if (kind === "assistant_reasoning_complete") {
    const existingIndex = reasoningId
      ? items.findIndex((item) => item.id === reasoningId)
      : items.findIndex((item) => item.kind === "reasoning" && item.meta === "running");
    if (existingIndex >= 0) {
      return items.map((item, index) =>
        index === existingIndex && item.kind === "reasoning"
          ? { ...item, body: body || item.body, meta: "complete" }
          : item
      );
    }
    return [
      ...items,
      {
        id: `event-${Date.now()}-${Math.random()}`,
        kind: "reasoning",
        title: "思考",
        body,
        meta: "complete"
      }
    ];
  }

  if (kind === "turn_completed") {
    let hasAssistantForTurn = false;
    let hasReasoningForTurn = false;
    const settledItems = items.map((item, index) => {
      if (item.kind === "reasoning" && (item.meta === "running" || item.id === reasoningId)) {
        hasReasoningForTurn = true;
        return { ...item, body: reasoning || item.body, meta: "complete" };
      }
      if (item.kind === "tool" && item.status === "running") {
        return { ...item, status: "success" as const };
      }
      if (item.kind === "assistant" && (item.id === assistantId || index === items.length - 1)) {
        hasAssistantForTurn = true;
        return { ...item, body: body || item.body, meta: hasToolCalls ? "intermediate" : "stream complete" };
      }
      if (item.kind === "assistant" && item.meta === "stream complete" && body && item.body === body) {
        hasAssistantForTurn = true;
      }
      if (item.kind === "reasoning" && reasoning && item.body === reasoning) {
        hasReasoningForTurn = true;
      }
      return item;
    });
    const fallbackItems: TimelineItem[] = [];
    if (reasoning && !hasReasoningForTurn) {
      fallbackItems.push({
        id: `event-${Date.now()}-${Math.random()}`,
        kind: "reasoning",
        title: "思考",
        body: reasoning,
        meta: "complete"
      });
    }
    if (body && !hasAssistantForTurn) {
      fallbackItems.push({
        id: `event-${Date.now()}-${Math.random()}`,
        kind: "assistant",
        title: "Yode",
        body,
        meta: hasToolCalls ? "intermediate" : "stream complete"
      });
    }
    return fallbackItems.length > 0 ? [...settledItems, ...fallbackItems] : settledItems;
  }

  if (kind === "error") {
    const filteredItems = items.filter((item) => item.id !== "retrying-attempt");
    const settledItems = filteredItems.map((item) => {
      if (item.kind === "reasoning" && item.meta === "running") {
        return { ...item, meta: "complete" };
      }
      if (item.kind === "tool" && item.status === "running") {
        return { ...item, status: "blocked" as const };
      }
      if (item.kind === "assistant" && item.meta !== "stream complete") {
        return { ...item, meta: "stream complete" };
      }
      return item;
    });
    return [
      ...settledItems,
      {
        id: `event-${Date.now()}-${Math.random()}`,
        kind: "assistant",
        title: "错误",
        body: body || "本轮执行失败，请稍后重试。",
        meta: "stream complete"
      }
    ];
  }

  return [...items, desktopEventToTimelineItem(payload, eventKind)];
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function mergeStreamingText(current: string, incoming: string): string {
  if (!incoming) return current;
  if (!current || incoming.startsWith(current)) return incoming;
  return `${current}${incoming}`;
}

function RunInspector({
  isProcessing,
  permissionMode,
  timelineItems
}: {
  isProcessing: boolean;
  permissionMode: string;
  timelineItems: TimelineItem[];
}) {
  const toolItems = timelineItems.filter((item) => item.kind === "tool");
  const completedToolItems = toolItems.filter((item) => item.status !== "running");
  return (
    <aside className="run-inspector" aria-label="运行详情">
      <div className="inspector-head">
        <span>TURN</span>
        <strong>{timelineItems.length} events</strong>
      </div>
      <div className="inspector-section">
        <div className="metric-row">
          <span>状态</span>
          <strong className={isProcessing ? "state-live" : ""}>{isProcessing ? "streaming" : "idle"}</strong>
        </div>
        <div className="metric-row">
          <span>权限</span>
          <strong>{permissionMode}</strong>
        </div>
        <div className="metric-row">
          <span>上下文</span>
          <strong>{timelineItems.length > 0 ? "active" : "empty"}</strong>
        </div>
        <div className="metric-row">
          <span>工具</span>
          <strong>{completedToolItems.length} / {toolItems.length}</strong>
        </div>
      </div>
      <div className="inspector-section">
        <span className="inspector-label">NEXT</span>
        <p>{isProcessing ? "正在等待模型或工具返回。" : "选择会话或发送消息继续。"}</p>
      </div>
    </aside>
  );
}
