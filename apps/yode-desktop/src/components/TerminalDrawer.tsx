import React, { useEffect, useMemo, useRef, useState } from "react";
import { Plus, TerminalSquare, Trash2, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

type TerminalOutputEvent = {
  sessionId: string;
  data: string;
};

type TerminalExitEvent = {
  sessionId: string;
  exitCode?: number | null;
};

type TerminalTab = {
  id: string;
  title: string;
  cwd: string;
  isRunning: boolean;
};

type TerminalSession = {
  tabs: TerminalTab[];
  activeTabId: string;
};

type TerminalDrawerProps = {
  isOpen: boolean;
  onClose: () => void;
  workspacePath: string;
  conversationId: string | null;
  height: number;
  onResizeStart: (event: React.PointerEvent) => void;
};

type XtermHandle = {
  terminal: Terminal;
  fitAddon: FitAddon;
  opened: boolean;
  backendOpened: boolean;
  dataDisposable?: { dispose: () => void };
};

function makeId(prefix: string) {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function createTab(workspacePath: string, index: number): TerminalTab {
  return {
    id: makeId("terminal"),
    title: `bash ${index}`,
    cwd: workspacePath,
    isRunning: false
  };
}

function createSession(workspacePath: string): TerminalSession {
  const firstTab = createTab(workspacePath, 1);
  return {
    tabs: [firstTab],
    activeTabId: firstTab.id
  };
}

function displayCwd(cwd: string) {
  if (!cwd) return "/";
  const homeMatch = cwd.match(/^\/Users\/[^/]+/) || cwd.match(/^\/home\/[^/]+/);
  if (!homeMatch) return cwd;
  const home = homeMatch[0];
  if (cwd === home) return "~";
  if (cwd.startsWith(`${home}/`)) return `~/${cwd.slice(home.length + 1)}`;
  return cwd;
}

function xtermTheme() {
  const root = getComputedStyle(document.documentElement);
  const value = (name: string, fallback: string) => root.getPropertyValue(name).trim() || fallback;
  return {
    background: value("--terminal-bg", value("--bg", "#111111")),
    foreground: value("--text", "#f8f8f2"),
    cursor: value("--accent", "#bd93f9"),
    selectionBackground: value("--terminal-selection", "rgba(189,147,249,0.18)"),
    selectionInactiveBackground: value("--terminal-selection-inactive", "rgba(189,147,249,0.10)"),
    black: value("--terminal-black", "#21222c"),
    red: value("--terminal-red", value("--error", "#ff5555")),
    green: value("--terminal-green", value("--success", "#50fa7b")),
    yellow: value("--terminal-yellow", value("--warning", "#f1fa8c")),
    blue: value("--terminal-blue", "#8be9fd"),
    magenta: value("--terminal-magenta", value("--accent", "#ff79c6")),
    cyan: value("--terminal-cyan", "#8be9fd"),
    white: value("--terminal-white", value("--text", "#f8f8f2")),
    brightBlack: value("--terminal-bright-black", "#6272a4"),
    brightRed: value("--terminal-bright-red", value("--error", "#ff6e6e")),
    brightGreen: value("--terminal-bright-green", value("--success", "#69ff94")),
    brightYellow: value("--terminal-bright-yellow", value("--warning", "#ffffa5")),
    brightBlue: value("--terminal-bright-blue", "#d6acff"),
    brightMagenta: value("--terminal-bright-magenta", value("--accent", "#ff92df")),
    brightCyan: value("--terminal-bright-cyan", "#a4ffff"),
    brightWhite: value("--terminal-bright-white", "#ffffff")
  };
}

function backendSessionId(sessionKey: string, tabId: string) {
  return `${sessionKey}::${tabId}`;
}

export function TerminalDrawer({ isOpen, onClose, workspacePath, conversationId, height, onResizeStart }: TerminalDrawerProps) {
  const sessionKey = conversationId || "__draft__";
  const [sessions, setSessions] = useState<Record<string, TerminalSession>>(() => ({
    [sessionKey]: createSession(workspacePath)
  }));
  const terminalHostsRef = useRef<Record<string, HTMLDivElement | null>>({});
  const xtermsRef = useRef<Record<string, XtermHandle>>({});
  const unlistenersRef = useRef<UnlistenFn[]>([]);
  const isTauri = "__TAURI_INTERNALS__" in window;

  useEffect(() => {
    setSessions((current) => {
      if (current[sessionKey]) return current;
      return {
        ...current,
        [sessionKey]: createSession(workspacePath)
      };
    });
  }, [sessionKey, workspacePath]);

  const currentSession = sessions[sessionKey];
  const tabs = currentSession?.tabs || [];
  const activeTabId = currentSession?.activeTabId || tabs[0]?.id || "";
  const activeTab = useMemo(
    () => tabs.find((tab) => tab.id === activeTabId) || tabs[0],
    [activeTabId, tabs]
  );

  const updateSession = (updater: (session: TerminalSession) => TerminalSession) => {
    setSessions((current) => {
      const session = current[sessionKey] || createSession(workspacePath);
      return {
        ...current,
        [sessionKey]: updater(session)
      };
    });
  };

  const updateTab = (id: string, updater: (tab: TerminalTab) => TerminalTab) => {
    updateSession((session) => ({
      ...session,
      tabs: session.tabs.map((tab) => (tab.id === id ? updater(tab) : tab))
    }));
  };

  const setActiveTabId = (id: string) => {
    updateSession((session) => ({
      ...session,
      activeTabId: id
    }));
  };

  const ensureXterm = (tab: TerminalTab) => {
    const key = backendSessionId(sessionKey, tab.id);
    const existing = xtermsRef.current[key];
    if (existing) return existing;

    const fitAddon = new FitAddon();
    const terminal = new Terminal({
      allowProposedApi: false,
      convertEol: true,
      cursorBlink: true,
      fontFamily: "Menlo, Monaco, Consolas, 'Liberation Mono', monospace",
      fontSize: 12,
      lineHeight: 1.35,
      scrollback: 10000,
      theme: xtermTheme()
    });
    terminal.loadAddon(fitAddon);
    const dataDisposable = terminal.onData((data) => {
      if (!isTauri) {
        terminal.write(data === "\r" ? "\r\n" : data);
        return;
      }
      void invoke("terminal_write", {
        request: {
          sessionId: key,
          data
        }
      }).catch(() => {});
    });

    const handle: XtermHandle = {
      terminal,
      fitAddon,
      opened: false,
      backendOpened: false,
      dataDisposable
    };
    xtermsRef.current[key] = handle;
    return handle;
  };

  const fitAndResize = (tab: TerminalTab) => {
    const key = backendSessionId(sessionKey, tab.id);
    const handle = xtermsRef.current[key];
    if (!handle?.opened) return;
    try {
      const host = terminalHostsRef.current[key];
      if (!host || host.clientWidth <= 0 || host.clientHeight <= 0) return;
      handle.fitAddon.fit();
      if (!isTauri) return;
      void invoke("terminal_resize", {
        request: {
          sessionId: key,
          cols: handle.terminal.cols,
          rows: handle.terminal.rows
        }
      }).catch(() => {});
    } catch {
      // Fit can throw while the drawer is animating or hidden.
    }
  };

  const openBackend = (tab: TerminalTab) => {
    const key = backendSessionId(sessionKey, tab.id);
    const handle = ensureXterm(tab);
    if (handle.backendOpened) return;
    handle.backendOpened = true;
    if (!isTauri) {
      handle.terminal.writeln("\x1b[2m这是非桌面环境的终端预览。打开 Tauri 桌面端后会连接真实 PTY。\x1b[0m");
      return;
    }
    void invoke("terminal_open", {
      request: {
        sessionId: key,
        cwd: tab.cwd,
        cols: handle.terminal.cols || 80,
        rows: handle.terminal.rows || 24
      }
    }).catch((err) => {
      handle.terminal.writeln(`\r\n\x1b[31m${String(err)}\x1b[0m`);
      updateTab(tab.id, (current) => ({ ...current, isRunning: false }));
      handle.backendOpened = false;
    });
  };

  useEffect(() => {
    if (!isOpen) return;
    const aliveKeys = new Set(tabs.map((tab) => backendSessionId(sessionKey, tab.id)));
    for (const tab of tabs) {
      const key = backendSessionId(sessionKey, tab.id);
      const host = terminalHostsRef.current[key];
      const handle = ensureXterm(tab);
      if (host && !handle.opened) {
        handle.terminal.open(host);
        handle.opened = true;
        window.setTimeout(() => {
          fitAndResize(tab);
          openBackend(tab);
        }, 240);
      } else if (host && handle.opened && !handle.backendOpened) {
        window.setTimeout(() => {
          fitAndResize(tab);
          openBackend(tab);
        }, 240);
      }
    }

    for (const [key, handle] of Object.entries(xtermsRef.current)) {
      if (key.startsWith(`${sessionKey}::`) && !aliveKeys.has(key)) {
        handle.dataDisposable?.dispose();
        handle.terminal.dispose();
        delete xtermsRef.current[key];
      }
    }
  }, [tabs, sessionKey, isTauri, isOpen]);

  useEffect(() => {
    let cancelled = false;
    const setup = async () => {
      if (!isTauri) return;
      const outputUnlisten = await listen<TerminalOutputEvent>("terminal-output", (event) => {
        const handle = xtermsRef.current[event.payload.sessionId];
        handle?.terminal.write(event.payload.data);
      });
      const exitUnlisten = await listen<TerminalExitEvent>("terminal-exit", (event) => {
        const handle = xtermsRef.current[event.payload.sessionId];
        if (handle) {
          handle.terminal.writeln("\r\n\x1b[2m[终端已退出]\x1b[0m");
          handle.backendOpened = false;
        }
      });
      if (cancelled) {
        outputUnlisten();
        exitUnlisten();
        return;
      }
      unlistenersRef.current = [outputUnlisten, exitUnlisten];
    };

    void setup();
    return () => {
      cancelled = true;
      for (const unlisten of unlistenersRef.current) {
        unlisten();
      }
      unlistenersRef.current = [];
    };
  }, [isTauri]);

  useEffect(() => {
    return () => {
      for (const [key, handle] of Object.entries(xtermsRef.current)) {
        handle.dataDisposable?.dispose();
        handle.terminal.dispose();
        if (isTauri) {
          void invoke("terminal_close", { sessionId: key }).catch(() => {});
        }
      }
      xtermsRef.current = {};
      terminalHostsRef.current = {};
    };
  }, [isTauri]);

  useEffect(() => {
    if (!isOpen || !activeTab) return;
    setTimeout(() => {
      fitAndResize(activeTab);
      xtermsRef.current[backendSessionId(sessionKey, activeTab.id)]?.terminal.focus();
    }, 230);
  }, [isOpen, activeTabId, sessionKey]);

  useEffect(() => {
    const handleResize = () => {
      if (isOpen && activeTab) fitAndResize(activeTab);
    };
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, [isOpen, activeTab, sessionKey]);

  useEffect(() => {
    for (const handle of Object.values(xtermsRef.current)) {
      handle.terminal.options.theme = xtermTheme();
    }
  }, [isOpen]);

  const addTab = () => {
    const next = createTab(workspacePath, tabs.length + 1);
    updateSession((session) => ({
      tabs: [...session.tabs, next],
      activeTabId: next.id
    }));
  };

  const closeTab = async (tabId: string) => {
    const key = backendSessionId(sessionKey, tabId);
    if (tabs.length <= 1) {
      const replacement = createTab(workspacePath, 1);
      updateSession(() => ({
        tabs: [replacement],
        activeTabId: replacement.id
      }));
    } else {
      const index = tabs.findIndex((tab) => tab.id === tabId);
      const nextTabs = tabs.filter((tab) => tab.id !== tabId);
      updateSession((session) => ({
        tabs: nextTabs,
        activeTabId:
          session.activeTabId === tabId
            ? nextTabs[Math.max(0, index - 1)]?.id || nextTabs[0]?.id || ""
            : session.activeTabId
      }));
    }

    const handle = xtermsRef.current[key];
    handle?.dataDisposable?.dispose();
    handle?.terminal.dispose();
    delete xtermsRef.current[key];
    try {
      await invoke("terminal_close", { sessionId: key });
    } catch {
      // Closing a local terminal tab should never interrupt the UI.
    }
  };

  const clearEmptySelection = (tabId: string) => {
    const key = backendSessionId(sessionKey, tabId);
    window.setTimeout(() => {
      const terminal = xtermsRef.current[key]?.terminal;
      if (!terminal) return;
      if (terminal.getSelection().trim().length === 0) {
        terminal.clearSelection();
      }
    }, 0);
  };

  return (
    <div
      className={`terminal-drawer ${isOpen ? "open" : ""}`}
      style={{ "--terminal-height": `${height}px` } as React.CSSProperties}
    >
      <div
        className="pane-resizer terminal-resizer"
        onPointerDown={onResizeStart}
        role="separator"
        aria-orientation="horizontal"
        title="拖动调整终端高度"
      />
      <div className="terminal-header">
        <div className="terminal-title">
          <TerminalSquare size={13} />
          <span>终端</span>
        </div>
        <div className="terminal-tabs" role="tablist" aria-label="终端会话">
          {tabs.map((tab, index) => (
            <button
              key={tab.id}
              className={`terminal-tab ${tab.id === activeTabId ? "active" : ""}`}
              onClick={() => setActiveTabId(tab.id)}
              type="button"
              role="tab"
              aria-selected={tab.id === activeTabId}
              title={`${tab.title} - ${displayCwd(tab.cwd)}`}
            >
              <span className={`terminal-tab-dot ${tab.isRunning ? "running" : ""}`} />
              <span className="terminal-tab-label">{tab.title || `bash ${index + 1}`}</span>
              <span
                className="terminal-tab-close"
                role="button"
                tabIndex={-1}
                title="关闭终端"
                onClick={(event) => {
                  event.stopPropagation();
                  void closeTab(tab.id);
                }}
              >
                <X size={11} />
              </span>
            </button>
          ))}
          <button className="terminal-add-tab" onClick={addTab} type="button" title="新增终端">
            <Plus size={13} />
          </button>
        </div>
        <div className="terminal-actions">
          <button
            className="terminal-action-btn"
            onClick={() => {
              if (activeTab) void closeTab(activeTab.id);
            }}
            type="button"
            title="删除当前终端"
          >
            <Trash2 size={13} />
          </button>
          <button className="terminal-action-btn" onClick={onClose} type="button" title="收起终端">
            <X size={14} />
          </button>
        </div>
      </div>
      <div className="terminal-body terminal-pty-body">
        {tabs.map((tab) => {
          const key = backendSessionId(sessionKey, tab.id);
          return (
            <div
              key={key}
              className={`terminal-pty-host ${tab.id === activeTabId ? "active" : ""}`}
              onPointerDown={(event) => event.stopPropagation()}
              onPointerUp={() => clearEmptySelection(tab.id)}
              onPointerCancel={() => clearEmptySelection(tab.id)}
              ref={(node) => {
                terminalHostsRef.current[key] = node;
              }}
            />
          );
        })}
      </div>
    </div>
  );
}
