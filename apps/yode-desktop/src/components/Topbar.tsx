import React, { useState, useRef, useMemo, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ChevronDown,
  MoreHorizontal,
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
  inspectorOpen: boolean;
  isProcessing: boolean;
  onToggleInspector: () => void;
  terminalOpen: boolean;
  onToggleTerminal: () => void;
  currentProvider: string;
  currentModel: string;
  onProviderChange: (provider: string) => void;
  onModelChange: (model: string) => void;
}

export function Topbar({
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
      <div className="title-stack" data-tauri-drag-region>
        <div className="session-heading" data-tauri-drag-region>{sessionTitle}</div>
        {workspacePath && (
          <div className="workspace-path" data-tauri-drag-region>
            <span data-tauri-drag-region>{workspacePath}</span>
            {currentBranch ? <span className="branch-name" data-tauri-drag-region>{currentBranch}</span> : null}
          </div>
        )}
      </div>
      <div className="runtime-strip" aria-label="运行状态" style={{ display: "flex", gap: "8px", alignItems: "center" }}>
        <DropdownPill
          icon={<TopbarProviderIcon id={currentProvider} />}
          label={providerName}
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

export function TopbarProviderIcon({ id }: { id: string }) {
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

interface DropdownPillProps {
  icon: React.ReactNode;
  label: string;
  options: { value: string; label: string }[];
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}

export function DropdownPill({
  icon,
  label,
  options,
  value,
  onChange,
  disabled
}: DropdownPillProps) {
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
