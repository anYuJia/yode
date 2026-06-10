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
  Check
} from "lucide-react";
import React, { useEffect, useMemo, useState, useRef } from "react";

import {
  Bootstrap,
  DesktopEvent,
  fallbackBootstrap,
  SessionSummary,
  sessions,
  TimelineItem,
  timeline,
  TurnAccepted
} from "./lib/mock";
import { SettingsShell } from "./components/SettingsShell";
import { TerminalDrawer } from "./components/TerminalDrawer";

type ViewMode = "chat" | "settings";

export function App() {
  const [bootstrap, setBootstrap] = useState<Bootstrap>(fallbackBootstrap);
  const [viewMode, setViewMode] = useState<ViewMode>(() => {
    return (localStorage.getItem("yode-view-mode") as ViewMode) || "chat";
  });
  const [appLang, setAppLang] = useState(() => localStorage.getItem("yode-language") || "zh");
  const [draft, setDraft] = useState("");
  const [sessionItems, setSessionItems] = useState(sessions);
  const [timelineItems, setTimelineItems] = useState<TimelineItem[]>(timeline);
  const [activeSessionId, setActiveSessionId] = useState<string>(sessions[0]?.id ?? "");
  const [inspectorOpen, setInspectorOpen] = useState(true);
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [messageQueue, setMessageQueue] = useState<string[]>([]);
  const [currentTurnId, setCurrentTurnId] = useState<string | null>(null);
  const [permissionMode, setPermissionMode] = useState<string>("default");

  const handlePermissionModeChange = (mode: string) => {
    setPermissionMode(mode);
    setBootstrap(prev => ({ ...prev, permissionMode: mode }));
    invoke("permission_mode_set", { mode }).catch(console.error);
  };

  useEffect(() => {
    const handleLangChange = (e: Event) => {
      const newLang = (e as CustomEvent).detail;
      setAppLang(newLang);
    };
    window.addEventListener("yode-language-change", handleLangChange);
    return () => window.removeEventListener("yode-language-change", handleLangChange);
  }, []);

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

  useEffect(() => {
    invoke<Bootstrap>("app_get_bootstrap")
      .then((nextBootstrap) => {
        setBootstrap(nextBootstrap);
        setPermissionMode(nextBootstrap.permissionMode);
        if (nextBootstrap.sessions.length > 0) {
          setSessionItems(nextBootstrap.sessions);
          setActiveSessionId(nextBootstrap.sessions[0].id);
        }
      })
      .catch(() => setBootstrap(fallbackBootstrap));
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

      if (kind === "turn_started") {
        setIsProcessing(true);
        if (outer) {
          setCurrentTurnId(outer.turnId);
        }
      } else if (kind === "turn_completed" || kind === "error") {
        setIsProcessing(false);
      }

      setTimelineItems((items) => [
        ...items,
        desktopEventToTimelineItem(
          (payload as any).kind ? (payload as DesktopEvent) : payload,
          (payload as any).kind ? undefined : (event as any).kind
        )
      ]);
    })
      .then((dispose) => {
        unlisten = dispose;
      })
      .catch(console.error);

    return () => unlisten?.();
  }, []);

  const activeSession = useMemo(
    () =>
      sessionItems.find((session) => session.id === activeSessionId) ??
      sessionItems[0] ??
      sessions[0],
    [activeSessionId, sessionItems]
  );

  async function handleCreateSession() {
    const session = await invoke<(typeof sessions)[number]>("sessions_create", {
      request: {
        title: "桌面端会话",
        projectRoot: bootstrap.workspacePath,
        provider: bootstrap.provider,
        model: bootstrap.model
      }
    });
    setSessionItems((items) => [
      { ...session, active: true },
      ...items.map((item) => ({ ...item, active: false }))
    ]);
    setActiveSessionId(session.id);
    setTimelineItems([]);
  }

  async function handleSendMessage() {
    if (!draft.trim() || !activeSession?.id) return;
    const content = draft.trim();
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
    const res = await invoke<TurnAccepted>("turn_send_message", {
      request: {
        sessionId: activeSession.id,
        content
      }
    });
    setCurrentTurnId(res.turnId);
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
          content: nextContent
        }
      }).then((res) => {
        setCurrentTurnId(res.turnId);
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
    setSessionItems(prev => prev.filter(s => s.id !== sessionId));
  };

  return (
    <main className="app-shell">
      <Sidebar
        sessions={sessionItems}
        viewMode={viewMode}
        onChangeView={handleSetViewMode}
        onCreateSession={handleCreateSession}
        onSelectSession={setActiveSessionId}
        onDeleteSession={handleDeleteSession}
      />
      <section className="workspace" style={{ position: "relative", overflow: "hidden" }}>
        <Topbar
          bootstrap={bootstrap}
          sessionTitle={activeSession.title}
          inspectorOpen={inspectorOpen}
          onToggleInspector={() => setInspectorOpen(!inspectorOpen)}
          terminalOpen={terminalOpen}
          onToggleTerminal={() => setTerminalOpen(!terminalOpen)}
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
          appLang={appLang}
        />
        <TerminalDrawer isOpen={terminalOpen} onClose={() => setTerminalOpen(false)} />
      </section>
    </main>
  );
}
function Sidebar({
  sessions,
  viewMode,
  onChangeView,
  onCreateSession,
  onSelectSession,
  onDeleteSession
}: {
  sessions: SessionSummary[];
  viewMode: ViewMode;
  onChangeView: (mode: ViewMode) => void;
  onCreateSession: () => void;
  onSelectSession: (sessionId: string) => void;
  onDeleteSession: (sessionId: string) => void;
}) {
  const lang = localStorage.getItem("yode-language") || "zh";
  const isZh = lang === "zh";
  const t = (zhText: string, enText: string) => isZh ? zhText : enText;

  // Track pinned sessions (e.g. initially s-1 is pinned)
  const [pinnedSessionIds, setPinnedSessionIds] = useState<string[]>(["s-1"]);
  const [deletingSessionId, setDeletingSessionId] = useState<string | null>(null);
  
  // Hover information popover state
  const [hoveredSessionId, setHoveredSessionId] = useState<string | null>(null);
  const [hoverPosition, setHoverPosition] = useState<{ top: number; left: number } | null>(null);
  const hoverTimerRef = useRef<number | null>(null);

  const handleMouseEnter = (sessionId: string, e: React.MouseEvent) => {
    // Clear any active timer
    if (hoverTimerRef.current) window.clearTimeout(hoverTimerRef.current);
    
    const rect = e.currentTarget.getBoundingClientRect();
    const pos = {
      top: rect.top,
      left: rect.right + 8
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

  // Group sessions into Pinned and Projects
  const pinnedSessions = sessions.filter(s => pinnedSessionIds.includes(s.id));
  const unpinnedSessions = sessions.filter(s => !pinnedSessionIds.includes(s.id));

  // Helper render method for a session item
  const renderSessionItem = (session: SessionSummary) => {
    const isPinned = pinnedSessionIds.includes(session.id);
    const isDeleting = deletingSessionId === session.id;

    return (
      <div
        className={`session-item-wrapper ${session.active ? "active" : ""}`}
        key={session.id}
        onMouseEnter={(e) => handleMouseEnter(session.id, e)}
        onMouseLeave={() => handleSessionMouseLeave(session.id)}
        style={{ position: "relative" }}
      >
        <button
          className={`session-button ${session.active ? "active" : ""}`}
          onClick={() => onSelectSession(session.id)}
          type="button"
          style={{ width: "100%", paddingRight: isDeleting ? "76px" : "32px", display: "flex", alignItems: "center", justifyContent: "space-between" }}
        >
          <span className="session-title" style={{ flex: 1, textOverflow: "ellipsis", overflow: "hidden", whiteSpace: "nowrap" }}>
            {session.title}
          </span>
          {!isDeleting && (
            <span className="session-time" style={{ fontSize: "10.5px", color: "var(--text-soft)", marginLeft: "4px" }}>
              {session.updatedAt}
            </span>
          )}
        </button>

        {/* Hover Actions / Confirm Buttons overlay */}
        {isDeleting ? (
          <div className="delete-confirm-overlay" style={{ position: "absolute", right: "6px", top: "50%", transform: "translateY(-50%)", display: "flex", gap: "4px" }}>
            <button
              onClick={(e) => handleConfirmDelete(session.id, e)}
              type="button"
              className="confirm-delete-btn"
              style={{
                background: "oklch(60% 0.16 30)",
                color: "#fff",
                border: "none",
                borderRadius: "4px",
                fontSize: "10.5px",
                fontWeight: "600",
                padding: "2px 8px",
                height: "22px",
                cursor: "pointer",
                display: "flex",
                alignItems: "center"
              }}
            >
              Confirm
            </button>
          </div>
        ) : (
          <div className="session-actions-overlay" style={{ display: "none", position: "absolute", right: "6px", top: "50%", transform: "translateY(-50%)", gap: "2px" }}>
            <button
              onClick={(e) => handleTogglePin(session.id, e)}
              type="button"
              className="action-icon-btn"
              style={{ background: "transparent", border: "none", cursor: "pointer", color: "var(--text-soft)", padding: "4px" }}
              title={isPinned ? t("取消置顶", "Unpin") : t("置顶", "Pin")}
            >
              {/* Pushpin SVG Icon */}
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ transform: isPinned ? "rotate(45deg)" : "none" }}>
                <line x1="18" y1="8" x2="22" y2="12"></line>
                <line x1="12" y1="2" x2="12" y2="6"></line>
                <path d="M12 6h8a2 2 0 0 1 2 2v2a2 2 0 0 1-2 2h-8M12 6H4a2 2 0 0 0-2 2v2a2 2 0 0 0 2 2h8"></path>
                <line x1="12" y1="12" x2="12" y2="22"></line>
              </svg>
            </button>
            <button
              onClick={(e) => handleDeleteClick(session.id, e)}
              type="button"
              className="action-icon-btn"
              style={{ background: "transparent", border: "none", cursor: "pointer", color: "var(--text-soft)", padding: "4px" }}
              title={t("删除", "Delete")}
            >
              {/* Trash SVG Icon */}
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <polyline points="3 6 5 6 21 6"></polyline>
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path>
              </svg>
            </button>
          </div>
        )}
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

      <button className="primary-action" onClick={onCreateSession} type="button">
        <MessageSquarePlus size={17} />
        {t("新对话", "New chat")}
      </button>

      <nav className="nav-block" aria-label="主导航">
        <NavButton icon={<Search size={16} />} label={t("搜索", "Search")} />
        <NavButton icon={<Code2 size={16} />} label={t("技能", "Skills")} />
        <NavButton icon={<Workflow size={16} />} label={t("插件", "Plugins")} />
        <NavButton icon={<Clock3 size={16} />} label={t("自动化", "Autopilot")} />
      </nav>

      {/* Pinned Section */}
      {pinnedSessions.length > 0 && (
        <div className="sidebar-section pinned-sessions" style={{ display: "flex", flexDirection: "column" }}>
          <div className="section-label">{t("已置顶", "Pinned")}</div>
          <div className="sessions-list" style={{ flex: "none" }}>
            {pinnedSessions.map(renderSessionItem)}
          </div>
        </div>
      )}

      {/* Projects Section */}
      <div className="sidebar-section sessions">
        <div className="section-label">{t("项目与对话", "Projects & Chats")}</div>
        <button className="project-button" type="button" style={{ marginBottom: "6px" }}>
          <Folder size={16} />
          <span>yode</span>
          <ChevronDown size={15} />
        </button>
        <div className="sessions-list">
          {unpinnedSessions.map(renderSessionItem)}
        </div>
      </div>

      {/* Hover info popover card */}
      {hoveredSessionId && hoverPosition && (
        <div
          className="session-popover"
          style={{
            position: "fixed",
            top: hoverPosition.top,
            left: hoverPosition.left,
            zIndex: 9999,
            width: "220px",
            background: "var(--panel-raised)",
            border: "1px solid var(--line-soft)",
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
                    <code>{s.project}</code>
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
        </div>
      )}

      <div className="sidebar-footer">
        <button
          className={`footer-button ${viewMode === "chat" ? "active" : ""}`}
          onClick={() => onChangeView("chat")}
          type="button"
          title={t("对话", "Chat")}
        >
          <Bot size={17} />
          {t("对话", "Chat")}
        </button>
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
  inspectorOpen,
  onToggleInspector,
  terminalOpen,
  onToggleTerminal
}: {
  bootstrap: Bootstrap;
  sessionTitle: string;
  inspectorOpen: boolean;
  onToggleInspector: () => void;
  terminalOpen: boolean;
  onToggleTerminal: () => void;
}) {
  return (
    <header className="topbar" data-tauri-drag-region>
      <div className="title-stack" data-tauri-drag-region>
        <div className="session-heading" data-tauri-drag-region>{sessionTitle}</div>
        <div className="workspace-path" data-tauri-drag-region>
          <span data-tauri-drag-region>{bootstrap.workspacePath}</span>
          <span>main</span>
        </div>
      </div>
      <div className="runtime-strip" aria-label="运行状态">
        <StatusPill icon={<Bot size={14} />} label={bootstrap.provider} tone="quiet" />
        <StatusPill icon={<Code2 size={14} />} label={bootstrap.model} tone="quiet" />
        <StatusPill icon={<ShieldCheck size={14} />} label={bootstrap.permissionMode} tone="quiet" />
        <StatusPill icon={<CircleDot size={14} />} label="运行中" tone="live" />
        <button className="icon-button" type="button" title="更多">
          <MoreHorizontal size={18} />
        </button>
        <button
          className={`icon-button ${terminalOpen ? "active" : ""}`}
          onClick={onToggleTerminal}
          type="button"
          title={terminalOpen ? "收起终端" : "打开终端"}
        >
          <TerminalSquare size={18} />
        </button>
        <button
          className="icon-button"
          onClick={onToggleInspector}
          type="button"
          title={inspectorOpen ? "收起运行详情" : "展开运行详情"}
        >
          {inspectorOpen ? <PanelRightClose size={18} /> : <PanelRight size={18} />}
        </button>
      </div>
    </header>
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
  appLang
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
  appLang: string;
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

  // Collapsible toggle for folded intermediate steps
  const [isCollapsed, setIsCollapsed] = useState(true);

  // Group items by turns or separate them. 
  // Intermediate steps: tool, reasoning, permission, boundary.
  // We want to hide them when NOT streaming, unless the user toggles to show them.
  const processedItems = useMemo(() => {
    const withoutActivePermission = timelineItems.filter((item) => item.kind !== "permission");

    if (isStreaming || !isCollapsed) {
      return withoutActivePermission;
    }

    // When NOT streaming and collapsed, filter out tool, reasoning, permission, boundary
    // and keep only user and final assistant responses.
    return withoutActivePermission.filter(item => item.kind === "user" || item.kind === "assistant");
  }, [timelineItems, isStreaming, isCollapsed]);

  const hiddenCount = timelineItems.length - processedItems.length;
  const activePermission = [...timelineItems]
    .reverse()
    .find((item): item is Extract<TimelineItem, { kind: "permission" }> => item.kind === "permission");

  return (
    <div className={`chat-layout ${inspectorOpen ? "" : "inspector-collapsed"}`}>
      <div className="conversation-column">
        <section className="timeline-panel" aria-label="会话时间线">
          <div className="timeline-header">
            <span>RUN LOG</span>
            <strong>desktop-scaffold</strong>
            <em>{timelineItems.length} events</em>
          </div>
          
          {processedItems.map((item, index) => {
            // Insert a divider toggle after the first "user" message if we hid any events
            const showToggle = !isStreaming && hiddenCount > 0 && item.kind === "user" && index === 0;

            return (
              <React.Fragment key={item.id}>
                <TimelineNode item={item} appLang={appLang} />
                {showToggle && (
                  <div 
                    onClick={() => setIsCollapsed(false)}
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
                      <span>已省略 {hiddenCount} 个中间思考与执行步骤...</span>
                    </div>
                    <span style={{ fontWeight: "600", fontSize: "11px", color: "var(--accent)" }}>展开详情</span>
                  </div>
                )}
              </React.Fragment>
            );
          })}

          {!isCollapsed && hiddenCount > 0 && (
            <div style={{ display: "flex", justifyContent: "center", marginBlock: "10px" }}>
              <button
                onClick={() => setIsCollapsed(true)}
                type="button"
                className="secondary-button"
                style={{ fontSize: "11.5px", paddingInline: "12px", height: "26px", color: "var(--text-soft)" }}
              >
                收起思考与执行步骤
              </button>
            </div>
          )}
        </section>
        {activePermission ? (
          <div className="permission-dock" aria-label="执行确认">
            <PermissionActions item={activePermission} appLang={appLang} />
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
        />
      </div>
      <RunInspector />
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
        <p>{item.body}</p>
        {item.kind === "tool" ? <ToolMeta item={item} /> : null}
        {item.kind === "permission" ? <PermissionActions item={item} appLang={appLang} /> : null}
      </div>
    </article>
  );
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
  appLang
}: {
  item: Extract<TimelineItem, { kind: "permission" }>;
  appLang: string;
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

function Composer({
  draft,
  onDraftChange,
  onSendMessage,
  isProcessing,
  onCancelMessage,
  permissionMode,
  onPermissionModeChange,
  appLang
}: {
  draft: string;
  onDraftChange: (value: string) => void;
  onSendMessage: () => void;
  isProcessing: boolean;
  onCancelMessage: () => void;
  permissionMode: string;
  onPermissionModeChange: (mode: string) => void;
  appLang: string;
}) {
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const isZh = appLang === "zh";

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

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setDropdownOpen(false);
      }
    }
    if (dropdownOpen) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [dropdownOpen]);

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
                    {isZh ? "如何授权 Codex 的操作？" : "How should Codex actions be approved?"}
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

          <button className="mode-chip" type="button" style={{ outline: "none", boxShadow: "none" }}>
            <Bot size={15} />
            sonnet
          </button>
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
  const tool = stringValue(inner?.tool) ?? "desktop";
  const title = stringValue(inner?.title) ?? "Yode";
  const body = stringValue(inner?.body) ?? "";
  const meta = stringValue(inner?.meta);
  const status = stringValue(inner?.status);

  if (kind === "permission" || kind === "tool_confirm_required") {
    return {
      id: `event-${Date.now()}-${Math.random()}`,
      kind: "permission",
      title: title || "需要授权确认",
      body: body || `工具 "${tool}" 请求执行。`,
      tool: tool,
      risk: meta || "中等风险",
      sessionId,
      turnId
    };
  }

  if (kind === "tool_started" || kind === "tool_result" || inner?.tool) {
    return {
      id: `event-${Date.now()}-${Math.random()}`,
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

  return {
    id: `event-${Date.now()}-${Math.random()}`,
    kind: "assistant",
    title,
    body,
    meta
  };
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function RunInspector() {
  return (
    <aside className="run-inspector" aria-label="运行详情">
      <div className="inspector-head">
        <span>TURN</span>
        <strong>mock-001</strong>
      </div>
      <div className="inspector-section">
        <div className="metric-row">
          <span>状态</span>
          <strong className="state-live">streaming</strong>
        </div>
        <div className="metric-row">
          <span>权限</span>
          <strong>default</strong>
        </div>
        <div className="metric-row">
          <span>上下文</span>
          <strong>31%</strong>
        </div>
        <div className="metric-row">
          <span>工具</span>
          <strong>2 / 3</strong>
        </div>
      </div>
      <div className="inspector-section">
        <span className="inspector-label">FILES</span>
        <button className="file-row" type="button">
          <FileCode2 size={14} />
          <span>apps/yode-desktop/src/App.tsx</span>
        </button>
        <button className="file-row" type="button">
          <FileCode2 size={14} />
          <span>src/styles/app.css</span>
        </button>
      </div>
      <div className="inspector-section">
        <span className="inspector-label">NEXT</span>
        <p>第 2 批接入 DesktopRuntime 和 EngineEvent bridge。</p>
      </div>
    </aside>
  );
}
