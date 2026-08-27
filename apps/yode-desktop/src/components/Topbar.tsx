import React, { useState, useRef, useMemo, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Bot,
  ChevronDown,
  GitBranch,
  PanelLeft,
  TerminalSquare,
  PanelRightClose,
  PanelRight,
  Check
} from "lucide-react";
import { Bootstrap } from "../lib/desktopTypes";
import { PROVIDERS_META } from "./settings/ProvidersSettings";
import {
  LLM_PROVIDERS_CHANGE_EVENT,
  providerDisplayNameFromStorage,
  providerOptionsFromStoredProviders
} from "../lib/llmProviderStorage";

interface TopbarProps {
  bootstrap: Bootstrap;
  sessionTitle: string;
  workspacePath: string | null;
  sidebarOpen: boolean;
  onToggleSidebar: () => void;
  inspectorOpen: boolean;
  isProcessing: boolean;
  onToggleInspector: () => void;
  terminalOpen: boolean;
  onToggleTerminal: () => void;
  currentProvider: string;
  currentModel: string;
  onConfigureProviders: () => void;
  onProviderChange: (provider: string) => void;
  onModelChange: (model: string) => void;
}

export function Topbar({
  bootstrap,
  sessionTitle,
  workspacePath,
  sidebarOpen,
  onToggleSidebar,
  inspectorOpen,
  isProcessing,
  onToggleInspector,
  terminalOpen,
  onToggleTerminal,
  currentProvider,
  currentModel,
  onConfigureProviders,
  onProviderChange,
  onModelChange
}: TopbarProps) {
  const [currentBranch, setCurrentBranch] = useState<string | null>(null);
  const [providerVersion, setProviderVersion] = useState(0);
  const providerOptions = useMemo(() => {
    return providerOptionsFromStoredProviders(PROVIDERS_META);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [providerVersion]);
  const providerName = useMemo(() => {
    return providerDisplayNameFromStorage(currentProvider, PROVIDERS_META);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentProvider, providerVersion]);

  useEffect(() => {
    const refreshProviders = () => setProviderVersion((version) => version + 1);
    window.addEventListener("storage", refreshProviders);
    window.addEventListener(LLM_PROVIDERS_CHANGE_EVENT, refreshProviders);
    return () => {
      window.removeEventListener("storage", refreshProviders);
      window.removeEventListener(LLM_PROVIDERS_CHANGE_EVENT, refreshProviders);
    };
  }, []);

  useEffect(() => {
    let alive = true;
    setCurrentBranch(null);
    if (!workspacePath || !("__TAURI_INTERNALS__" in window)) return;

    invoke<string | null>("git_current_branch", {
      workspacePath,
      workspace_path: workspacePath
    })
      .then((branch) => {
        if (alive) setCurrentBranch(branch);
      })
      .catch(() => {
        if (alive) setCurrentBranch(null);
      });

    return () => {
      alive = false;
    };
  }, [workspacePath]);

  return (
    <header className="topbar" data-tauri-drag-region>
      <button
        className={`icon-button topbar-sidebar-button ${sidebarOpen ? "active" : ""}`}
        onClick={onToggleSidebar}
        data-tauri-no-drag
        type="button"
        title={`${sidebarOpen ? "收起" : "展开"}侧栏 (⌘B)`}
        aria-label={sidebarOpen ? "收起侧栏" : "展开侧栏"}
        aria-pressed={sidebarOpen}
        aria-controls="app-sidebar"
      >
        <PanelLeft size={18} />
      </button>
      <div className="title-stack" data-tauri-drag-region>
        <div className="session-heading" data-tauri-drag-region>{sessionTitle}</div>
        {workspacePath && (
          <div className="workspace-path" data-tauri-drag-region>
            <span data-tauri-drag-region>{workspacePath}</span>
            {currentBranch ? (
              <span className="branch-name" data-tauri-drag-region title={currentBranch}>
                <GitBranch size={10} aria-hidden="true" />
                {currentBranch}
              </span>
            ) : null}
          </div>
        )}
      </div>
      <div className="runtime-strip" aria-label="运行状态">
        <StatusPill
          icon={<span className={`runtime-status-dot ${isProcessing ? "is-live" : ""}`} aria-hidden="true" />}
          label={isProcessing ? "运行中" : "就绪"}
          tone={isProcessing ? "live" : "quiet"}
        />
        <DropdownPill
          icon={<TopbarProviderIcon id={currentProvider} />}
          label={providerName || currentProvider || bootstrap.provider || "选择提供商"}
          value={currentProvider}
          options={providerOptions}
          onEmptyClick={onConfigureProviders}
          onChange={onProviderChange}
        />
        <button
          className={`icon-button ${terminalOpen ? "active" : ""}`}
          onClick={onToggleTerminal}
          data-tauri-no-drag
          type="button"
          title={terminalOpen ? "收起终端" : "打开终端"}
          aria-label={terminalOpen ? "收起终端" : "打开终端"}
          aria-pressed={terminalOpen}
          aria-controls="terminal-drawer"
        >
          <TerminalSquare size={18} />
        </button>
        <button
          className="icon-button"
          onClick={onToggleInspector}
          data-tauri-no-drag
          type="button"
          title={inspectorOpen ? "收起运行详情" : "展开运行详情"}
          aria-label={inspectorOpen ? "收起运行详情" : "展开运行详情"}
          aria-pressed={inspectorOpen}
          aria-controls="run-inspector"
        >
          {inspectorOpen ? <PanelRightClose size={18} /> : <PanelRight size={18} />}
        </button>
      </div>
    </header>
  );
}

export function TopbarProviderIcon({ id }: { id: string }) {
  const [failed, setFailed] = useState(false);
  if (!id || failed) {
    return (
      <span className="provider-icon-fallback" aria-hidden="true">
        {id ? id.slice(0, 1).toUpperCase() : <Bot size={11} />}
      </span>
    );
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

interface DropdownPillProps {
  icon: React.ReactNode;
  label: string;
  options: { value: string; label: string }[];
  value: string;
  onChange: (value: string) => void;
  onEmptyClick?: () => void;
  disabled?: boolean;
}

export function DropdownPill({
  icon,
  label,
  options,
  value,
  onChange,
  onEmptyClick,
  disabled
}: DropdownPillProps) {
  const [isOpen, setIsOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const hasOptions = options.length > 0;

  const focusOption = (position: "first" | "last" | "next" | "previous") => {
    const optionElements = Array.from(
      ref.current?.querySelectorAll<HTMLButtonElement>('[role="option"]') ?? []
    );
    if (optionElements.length === 0) return;
    const currentIndex = optionElements.indexOf(document.activeElement as HTMLButtonElement);
    const targetIndex = position === "first"
      ? 0
        : position === "last"
          ? optionElements.length - 1
          : position === "next"
            ? currentIndex < 0 ? 0 : (currentIndex + 1) % optionElements.length
            : currentIndex < 0
              ? optionElements.length - 1
              : (currentIndex - 1 + optionElements.length) % optionElements.length;
    optionElements[targetIndex]?.focus();
  };

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, []);

  return (
    <div
      ref={ref}
      style={{ position: "relative" }}
      onKeyDown={(event) => {
        if (event.key === "Escape" && isOpen) {
          event.preventDefault();
          event.stopPropagation();
          setIsOpen(false);
          window.requestAnimationFrame(() => triggerRef.current?.focus());
          return;
        }
        if (!hasOptions || !["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
        event.preventDefault();
        if (!isOpen) {
          setIsOpen(true);
          window.requestAnimationFrame(() => focusOption(event.key === "ArrowUp" || event.key === "End" ? "last" : "first"));
          return;
        }
        if (event.key === "Home") focusOption("first");
        else if (event.key === "End") focusOption("last");
        else focusOption(event.key === "ArrowDown" ? "next" : "previous");
      }}
    >
      <button
        ref={triggerRef}
        type="button"
        data-tauri-no-drag
        disabled={disabled}
        onClick={() => {
          if (!hasOptions) {
            onEmptyClick?.();
            return;
          }
          setIsOpen(!isOpen);
        }}
        className="status-pill quiet"
        aria-label={hasOptions ? `模型提供商：${label}` : "未配置模型提供商，前往设置"}
        aria-haspopup={hasOptions ? "listbox" : undefined}
        aria-expanded={hasOptions ? isOpen : undefined}
        aria-controls={hasOptions ? "topbar-provider-listbox" : undefined}
        title={hasOptions ? `模型提供商：${label}` : "前往设置添加模型提供商"}
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
        {!disabled && hasOptions && <ChevronDown size={11} style={{ opacity: 0.7, transform: isOpen ? "rotate(180deg)" : "none", transition: "transform 150ms" }} />}
      </button>

      {isOpen && hasOptions && (
        <div
          id="topbar-provider-listbox"
          className="context-dropdown"
          role="listbox"
          aria-label="模型提供商"
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
                role="option"
                aria-selected={isSelected}
                className={`context-option ${isSelected ? "selected" : ""}`}
                onClick={() => {
                  onChange(opt.value);
                  setIsOpen(false);
                  window.requestAnimationFrame(() => triggerRef.current?.focus());
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

export function StatusPill({
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
