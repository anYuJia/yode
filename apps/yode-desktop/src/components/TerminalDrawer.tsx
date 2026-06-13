import React, { useEffect, useMemo, useRef, useState } from "react";
import { Plus, TerminalSquare, Trash2, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

type TerminalRunResponse = {
  output: string;
  cwd: string;
  exitCode: number;
};

type TerminalLine = {
  id: string;
  kind: "system" | "command" | "output" | "error";
  text: string;
  prompt?: string;
  exitCode?: number;
};

type TerminalTab = {
  id: string;
  title: string;
  cwd: string;
  input: string;
  isRunning: boolean;
  lines: TerminalLine[];
  history: string[];
  historyIndex: number | null;
};

type TerminalDrawerProps = {
  isOpen: boolean;
  onClose: () => void;
  workspacePath: string;
};

const SESSION_START = "Yode Terminal Session Started. Ready to execute local tasks.";

function makeId(prefix: string) {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function createTab(workspacePath: string, index: number): TerminalTab {
  return {
    id: makeId("terminal"),
    title: `bash ${index}`,
    cwd: workspacePath,
    input: "",
    isRunning: false,
    history: [],
    historyIndex: null,
    lines: [
      {
        id: makeId("line"),
        kind: "system",
        text: SESSION_START
      }
    ]
  };
}

function basename(path: string) {
  return path.split("/").filter(Boolean).pop() || "yode";
}

function displayCwd(cwd: string) {
  if (!cwd) return "/";
  const homeMatch = cwd.match(/^\/Users\/[^/]+/);
  if (!homeMatch) return cwd;
  const home = homeMatch[0];
  if (cwd === home) return "~";
  if (cwd.startsWith(`${home}/`)) return `~/${cwd.slice(home.length + 1)}`;
  return cwd;
}

function promptFor(cwd: string, workspacePath: string) {
  return `yode@local:${displayCwd(cwd)}$`;
}

function titleForCwd(cwd: string, workspacePath: string) {
  if (cwd === workspacePath) return "bash";
  return basename(cwd);
}

function splitOutput(text: string) {
  return text.replace(/\r\n/g, "\n").split("\n");
}

export function TerminalDrawer({ isOpen, onClose, workspacePath }: TerminalDrawerProps) {
  const [tabs, setTabs] = useState<TerminalTab[]>(() => [createTab(workspacePath, 1)]);
  const [activeTabId, setActiveTabId] = useState(() => tabs[0]?.id || "");
  const bodyElRef = useRef<HTMLDivElement>(null);
  const inputElRef = useRef<HTMLInputElement>(null);

  const activeTab = useMemo(
    () => tabs.find((tab) => tab.id === activeTabId) || tabs[0],
    [activeTabId, tabs]
  );

  useEffect(() => {
    if (bodyElRef.current) {
      bodyElRef.current.scrollTop = bodyElRef.current.scrollHeight;
    }
  }, [activeTab?.lines, activeTab?.isRunning, activeTabId, isOpen]);

  useEffect(() => {
    if (isOpen) {
      window.setTimeout(() => inputElRef.current?.focus(), 0);
    }
  }, [isOpen, activeTabId]);

  const updateTab = (id: string, updater: (tab: TerminalTab) => TerminalTab) => {
    setTabs((current) => current.map((tab) => (tab.id === id ? updater(tab) : tab)));
  };

  const addTab = () => {
    const next = createTab(workspacePath, tabs.length + 1);
    setTabs((current) => [...current, next]);
    setActiveTabId(next.id);
  };

  const closeTab = async (tabId: string) => {
    if (tabs.length <= 1) {
      const replacement = createTab(workspacePath, 1);
      setTabs([replacement]);
      setActiveTabId(replacement.id);
      try {
        await invoke("terminal_close", { sessionId: tabId });
      } catch {
        // Closing a local terminal tab should never interrupt the UI.
      }
      return;
    }

    const index = tabs.findIndex((tab) => tab.id === tabId);
    const nextTabs = tabs.filter((tab) => tab.id !== tabId);
    setTabs(nextTabs);
    if (activeTabId === tabId) {
      setActiveTabId(nextTabs[Math.max(0, index - 1)]?.id || nextTabs[0]?.id || "");
    }

    try {
      await invoke("terminal_close", { sessionId: tabId });
    } catch {
      // Closing a local terminal tab should never interrupt the UI.
    }
  };

  const runCommand = async (tab: TerminalTab, rawCommand: string) => {
    const command = rawCommand.trim();
    if (!command || tab.isRunning) return;

    const prompt = promptFor(tab.cwd, workspacePath);
    const commandLine: TerminalLine = {
      id: makeId("line"),
      kind: "command",
      text: command,
      prompt
    };

    updateTab(tab.id, (current) => ({
      ...current,
      input: "",
      isRunning: command !== "clear" && command !== "exit",
      history: current.history[current.history.length - 1] === command ? current.history : [...current.history, command],
      historyIndex: null,
      lines: command === "clear" ? [] : [...current.lines, commandLine]
    }));

    if (command === "clear") return;
    if (command === "exit") {
      await closeTab(tab.id);
      return;
    }

    try {
      const response = await invoke<TerminalRunResponse>("terminal_run", {
        request: {
          sessionId: tab.id,
          command
        }
      });
      const outputLines = response.output
        ? splitOutput(response.output).map<TerminalLine>((line) => ({
            id: makeId("line"),
            kind: response.exitCode === 0 ? "output" : "error",
            text: line,
            exitCode: response.exitCode
          }))
        : [];
      updateTab(tab.id, (current) => ({
        ...current,
        cwd: response.cwd,
        title: titleForCwd(response.cwd, workspacePath),
        isRunning: false,
        lines: [...current.lines, ...outputLines]
      }));
    } catch (err) {
      updateTab(tab.id, (current) => ({
        ...current,
        isRunning: false,
        lines: [
          ...current.lines,
          {
            id: makeId("line"),
            kind: "error",
            text: String(err),
            exitCode: 1
          }
        ]
      }));
    }
  };

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    if (activeTab) {
      void runCommand(activeTab, activeTab.input);
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (!activeTab) return;
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    if (activeTab.history.length === 0) return;

    event.preventDefault();
    const lastIndex = activeTab.history.length - 1;
    const nextIndex =
      event.key === "ArrowUp"
        ? activeTab.historyIndex === null
          ? lastIndex
          : Math.max(0, activeTab.historyIndex - 1)
        : activeTab.historyIndex === null
          ? null
          : activeTab.historyIndex >= lastIndex
            ? null
            : activeTab.historyIndex + 1;

    updateTab(activeTab.id, (tab) => ({
      ...tab,
      historyIndex: nextIndex,
      input: nextIndex === null ? "" : tab.history[nextIndex]
    }));
  };

  return (
    <div className={`terminal-drawer ${isOpen ? "open" : ""}`}>
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
      <div className="terminal-body" ref={bodyElRef} onClick={() => inputElRef.current?.focus()}>
        {activeTab?.lines.map((line) => (
          <div
            key={line.id}
            className={`terminal-line terminal-line-${line.kind} ${
              line.exitCode !== undefined && line.exitCode !== 0 ? "text-error" : ""
            }`}
          >
            {line.prompt && <span className="terminal-prompt">{line.prompt}</span>}
            <span>{line.text}</span>
          </div>
        ))}
        {activeTab && (
          <form onSubmit={handleSubmit} className="terminal-line active-line">
            <span className="terminal-prompt">{promptFor(activeTab.cwd, workspacePath)}</span>
            <input
              ref={inputElRef}
              type="text"
              value={activeTab.input}
              onChange={(event) => updateTab(activeTab.id, (tab) => ({ ...tab, input: event.target.value }))}
              onKeyDown={handleKeyDown}
              className="terminal-real-input"
              disabled={activeTab.isRunning}
              placeholder={activeTab.isRunning ? "running..." : ""}
              autoFocus={isOpen}
              spellCheck={false}
            />
          </form>
        )}
      </div>
    </div>
  );
}
