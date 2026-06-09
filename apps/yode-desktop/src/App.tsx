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
  Download
} from "lucide-react";
import { useEffect, useMemo, useState, useRef } from "react";

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
  const [draft, setDraft] = useState("");
  const [sessionItems, setSessionItems] = useState(sessions);
  const [timelineItems, setTimelineItems] = useState<TimelineItem[]>(timeline);
  const [activeSessionId, setActiveSessionId] = useState<string>(sessions[0]?.id ?? "");
  const [inspectorOpen, setInspectorOpen] = useState(true);
  const [terminalOpen, setTerminalOpen] = useState(false);

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
      } else {
        shell.classList.remove("translucent-sidebar");
      }
    });
  }, [viewMode]);

  useEffect(() => {
    invoke<Bootstrap>("app_get_bootstrap")
      .then((nextBootstrap) => {
        setBootstrap(nextBootstrap);
        if (nextBootstrap.sessions.length > 0) {
          setSessionItems(nextBootstrap.sessions);
          setActiveSessionId(nextBootstrap.sessions[0].id);
        }
      })
      .catch(() => setBootstrap(fallbackBootstrap));
  }, []);

  useEffect(() => {
    return void listen<DesktopEvent>("desktop-event", (event) => {
      const payload = event.payload;
      setTimelineItems((items) => [
        ...items,
        desktopEventToTimelineItem(
          (payload as any).kind ? (payload as DesktopEvent) : payload,
          (payload as any).kind ? undefined : (event as any).kind
        )
      ]);
    });
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
    setTimelineItems((items) => [
      ...items,
      {
        id: `local-${Date.now()}`,
        kind: "user",
        title: "用户",
        body: content,
        meta: "desktop"
      }
    ]);
    await invoke<TurnAccepted>("turn_send_message", {
      request: {
        sessionId: activeSession.id,
        content
      }
    });
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

  return (
    <main className="app-shell">
      <Sidebar
        sessions={sessionItems}
        viewMode={viewMode}
        onChangeView={handleSetViewMode}
        onCreateSession={handleCreateSession}
        onSelectSession={setActiveSessionId}
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
  onSelectSession
}: {
  sessions: SessionSummary[];
  viewMode: ViewMode;
  onChangeView: (mode: ViewMode) => void;
  onCreateSession: () => void;
  onSelectSession: (sessionId: string) => void;
}) {
  return (
    <aside className="sidebar">
      <div className="brand-row" data-tauri-drag-region>
        <div className="brand-mark">Y</div>
        <div data-tauri-drag-region>
          <div className="brand-title" data-tauri-drag-region>Yode</div>
          <div className="brand-subtitle" data-tauri-drag-region>local agent runtime</div>
        </div>
      </div>

      <button className="primary-action" onClick={onCreateSession} type="button">
        <MessageSquarePlus size={17} />
        新对话
      </button>

      <nav className="nav-block" aria-label="主导航">
        <NavButton icon={<Search size={16} />} label="搜索" />
        <NavButton icon={<Code2 size={16} />} label="技能" />
        <NavButton icon={<Workflow size={16} />} label="插件" />
        <NavButton icon={<Clock3 size={16} />} label="自动化" />
      </nav>

      <div className="sidebar-section sessions">
        <div className="section-label">项目与对话</div>
        <button className="project-button" type="button" style={{ marginBottom: "6px" }}>
          <Folder size={16} />
          <span>yode</span>
          <ChevronDown size={15} />
        </button>
        <div className="sessions-list">
          {sessions.map((session) => (
            <button
              className={`session-button ${session.active ? "active" : ""}`}
              key={session.id}
              onClick={() => onSelectSession(session.id)}
              type="button"
            >
              <span className="session-title">{session.title}</span>
              <span className="session-meta">{session.project} · {session.updatedAt}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="sidebar-footer">
        <button
          className={`footer-button ${viewMode === "chat" ? "active" : ""}`}
          onClick={() => onChangeView("chat")}
          type="button"
          title="对话"
        >
          <Bot size={17} />
          对话
        </button>
        <button
          className={`footer-button ${viewMode === "settings" ? "active" : ""}`}
          onClick={() => onChangeView("settings")}
          type="button"
          title="设置"
        >
          <Settings size={17} />
          设置
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
  inspectorOpen
}: {
  draft: string;
  timelineItems: TimelineItem[];
  onDraftChange: (value: string) => void;
  onSendMessage: () => void;
  inspectorOpen: boolean;
}) {
  return (
    <div className={`chat-layout ${inspectorOpen ? "" : "inspector-collapsed"}`}>
      <div className="conversation-column">
        <section className="timeline-panel" aria-label="会话时间线">
          <div className="timeline-header">
            <span>RUN LOG</span>
            <strong>desktop-scaffold</strong>
            <em>7 events</em>
          </div>
          {timelineItems.map((item) => (
            <TimelineNode item={item} key={item.id} />
          ))}
        </section>
        <Composer draft={draft} onDraftChange={onDraftChange} onSendMessage={onSendMessage} />
      </div>
      <RunInspector />
    </div>
  );
}

function TimelineNode({ item }: { item: TimelineItem }) {
  if (item.kind === "boundary") {
    return (
      <div className="boundary-node">
        <span>{item.title}</span>
        <p>{item.body}</p>
      </div>
    );
  }

  const icon =
    item.kind === "tool" ? (
      <Hammer size={18} />
    ) : item.kind === "permission" ? (
      <ShieldCheck size={18} />
    ) : item.kind === "user" ? (
      <Command size={18} />
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
        {item.kind === "permission" ? <PermissionActions item={item} /> : null}
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
  item
}: {
  item: Extract<TimelineItem, { kind: "permission" }>;
}) {
  return (
    <div className="permission-box">
      <div>
        <span>需要确认</span>
        <code>{item.tool}</code>
        <strong>{item.risk}</strong>
      </div>
      <div className="permission-actions">
        <button className="secondary-button" type="button">
          拒绝
        </button>
        <button className="primary-button" type="button">
          允许
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
  onSendMessage
}: {
  draft: string;
  onDraftChange: (value: string) => void;
  onSendMessage: () => void;
}) {
  return (
    <footer className="composer">
      <textarea
        aria-label="消息"
        placeholder="输入仓库任务..."
        value={draft}
        onChange={(event) => onDraftChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
            event.preventDefault();
            onSendMessage();
          }
        }}
      />
      <div className="composer-toolbar">
        <div className="composer-tools">
          <button className="icon-button" type="button" title="附件">
            <Paperclip size={17} />
          </button>
          <button className="mode-chip" type="button">
            <ShieldCheck size={15} />
            Default
          </button>
          <button className="mode-chip" type="button">
            <Bot size={15} />
            sonnet
          </button>
        </div>
        <div className="composer-actions">
          <button className="icon-button" type="button" title="停止">
            <Pause size={17} />
          </button>
          <button className="send-button" onClick={onSendMessage} type="button" title="发送">
            <Send size={17} />
          </button>
        </div>
      </div>
    </footer>
  );
}

function desktopEventToTimelineItem(
  payload: DesktopEvent["payload"],
  eventKind?: string
): TimelineItem {
  const kind = eventKind ?? stringValue(payload.kind) ?? stringValue(payload.type);
  const tool = stringValue(payload.tool) ?? "desktop";
  const title = stringValue(payload.title) ?? "Yode";
  const body = stringValue(payload.body) ?? "";
  const meta = stringValue(payload.meta);
  const status = stringValue(payload.status);

  if (kind === "tool_started" || kind === "tool_result" || payload.tool) {
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
