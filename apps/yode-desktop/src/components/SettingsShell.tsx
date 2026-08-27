import React, { useRef, useState, useEffect, useCallback } from "react";
import {
  Settings,
  Eye,
  Sliders,
  Sparkles,
  Command,
  MonitorPlay,
  TerminalSquare,
  Globe,
  Fingerprint,
  GitBranch,
  Workflow,
  Code2,
  Folder,
  Archive,
  Bot,
  Download,
  ArrowLeft,
  Search,
  X
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { Bootstrap } from "../lib/desktopTypes";

import {
  DesktopSettingsStatus,
  isTauriRuntime,
  loadDesktopSettingsStatus,
  restoreDesktopSettings
} from "../lib/desktopSettings";
import { SettingsFileWarning } from "./SettingsFileWarning";

import {
  AppearanceSettings,
  ConfigurationSettings,
  PersonalizationSettings,
  KeyboardShortcutsSettings
} from "./settings/PersonalSettings";
import {
  ComputerUseSettingsSettings,
  McpSettingsSettings,
  BrowserSettingsSettings
} from "./settings/IntegrationSettings";
import {
  HooksSettingsSettings,
  GitSettingsSettings,
  EnvironmentsSettingsSettings,
  WorktreesSettingsSettings
} from "./settings/CodingSettings";
import {
  ArchivedChatsSettingsSettings
} from "./settings/ArchivedChatsSettings";
import { ProvidersSettings } from "./settings/ProvidersSettings";
import { GeneralSettings } from "./settings/GeneralSettings";

const MANAGED_DIALOG_SELECTOR = '[data-settings-dialog="true"][role="dialog"]';
const MANAGED_DIALOG_FOCUSABLE = [
  "button:not([disabled])",
  "[href]",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])"
].join(",");
import { AboutSettings } from "./settings/AboutSettings";
import {
  LANGUAGE_CHANGE_EVENT,
  languageFromChangeEvent,
  loadAppLanguage
} from "../lib/appearanceSettings";
import {
  loadActiveSettingsTab,
  saveActiveSettingsTab,
  useAppUiStore
} from "../lib/appUiStore";

function clampNumber(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function SettingsShell({ bootstrap, onClose }: { bootstrap: Bootstrap; onClose: () => void }) {
  const [activeTab, setActiveTab] = useState(() => loadActiveSettingsTab());
  const [searchQuery, setSearchQuery] = useState("");
  const sidebarWidth = useAppUiStore((state) => state.settingsSidebarWidth);
  const setSettingsSidebarWidth = useAppUiStore((state) => state.setSettingsSidebarWidth);
  const [draggingSidebar, setDraggingSidebar] = useState(false);
  const sidebarDragRef = useRef<{ startX: number; startWidth: number; target: Element | null; pointerId: number | null } | null>(null);
  const settingsRootRef = useRef<HTMLDivElement>(null);
  const dialogTriggerRef = useRef<HTMLElement | null>(null);
  const lastStableFocusRef = useRef<HTMLElement | null>(null);

  const handleSetActiveTab = (tab: string) => {
    setActiveTab(saveActiveSettingsTab(tab));
    setSearchQuery("");
  };

  const [currentLang, setCurrentLang] = useState(() => loadAppLanguage());
  const isZh = currentLang === "zh";

  const t = (zhText: string, enText: string) => {
    return isZh ? zhText : enText;
  };

  // 桌面设置文件加载状态：损坏/不可读时设置页必须明确提示并提供可访问操作。
  const [settingsFileStatus, setSettingsFileStatus] = useState<DesktopSettingsStatus | null>(null);

  const refreshSettingsFileStatus = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setSettingsFileStatus(await loadDesktopSettingsStatus());
  }, []);

  useEffect(() => {
    void refreshSettingsFileStatus();
  }, [refreshSettingsFileStatus]);

  const handleOpenSettingsFile = () => {
    if (!settingsFileStatus) return;
    void invoke("open_target", {
      request: { target: null, path: settingsFileStatus.path }
    }).catch((err) => console.error(err));
  };

  const handleRestoreSettingsFile = async () => {
    const confirmed = window.confirm(
      t(
        "恢复默认设置会将当前损坏的设置文件备份为 .bak 文件，并生成一份全新的空配置。确定继续吗？",
        "Restoring defaults will back up the corrupted settings file as a .bak file and create a fresh empty configuration. Continue?"
      )
    );
    if (!confirmed) return;
    const restored = await restoreDesktopSettings();
    if (restored && restored.loaded) {
      await refreshSettingsFileStatus();
    }
  };

  useEffect(() => {
    const handleLangChange = (e: Event) => {
      setCurrentLang(languageFromChangeEvent(e));
    };
    window.addEventListener(LANGUAGE_CHANGE_EVENT, handleLangChange);
    return () => window.removeEventListener(LANGUAGE_CHANGE_EVENT, handleLangChange);
  }, []);

  useEffect(() => {
    if (!draggingSidebar) return;

    const releaseCapture = () => {
      const drag = sidebarDragRef.current;
      if (drag?.target && drag.pointerId !== null && "releasePointerCapture" in drag.target) {
        try {
          (drag.target as HTMLElement).releasePointerCapture(drag.pointerId);
        } catch {
          // pointerup/cancel 后浏览器可能已经自动释放。
        }
      }
    };

    const handlePointerMove = (event: PointerEvent) => {
      const drag = sidebarDragRef.current;
      if (!drag) return;
      setSettingsSidebarWidth(clampNumber(drag.startWidth + event.clientX - drag.startX, 180, 340));
    };

    const stopDragging = () => {
      releaseCapture();
      sidebarDragRef.current = null;
      setDraggingSidebar(false);
    };

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", stopDragging);
    window.addEventListener("pointercancel", stopDragging);
    window.addEventListener("blur", stopDragging);
    return () => {
      releaseCapture();
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", stopDragging);
      window.removeEventListener("pointercancel", stopDragging);
      window.removeEventListener("blur", stopDragging);
    };
  }, [draggingSidebar, setSettingsSidebarWidth]);

  const beginSidebarDrag = (event: React.PointerEvent) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // 某些嵌入式 WebView 可能不支持 capture，窗口级监听仍可完成拖拽。
    }
    sidebarDragRef.current = {
      startX: event.clientX,
      startWidth: sidebarWidth,
      target: event.currentTarget,
      pointerId: event.pointerId
    };
    setDraggingSidebar(true);
  };

  const resizeSettingsSidebarWithKeyboard = (event: React.KeyboardEvent) => {
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const step = event.shiftKey ? 32 : 12;
    const next = event.key === "Home"
      ? 180
      : event.key === "End"
        ? 340
        : clampNumber(
          sidebarWidth + (event.key === "ArrowRight" || event.key === "ArrowDown" ? step : -step),
          180,
          340
        );
    setSettingsSidebarWidth(next);
  };

  const categories = [
    {
      title: t("个人设置", "Personal"),
      items: [
        { id: "常规", label: t("常规", "General"), description: t("语言、启动与对话行为", "Language, startup, and chat behavior"), icon: Settings },
        { id: "外观", label: t("外观", "Appearance"), description: t("主题、字体、密度与辅助功能", "Theme, typography, density, and accessibility"), icon: Eye },
        { id: "配置", label: t("配置", "Configuration"), description: t("审批策略、沙箱与工作区依赖", "Approvals, sandboxing, and workspace dependencies"), icon: Sliders },
        { id: "模型提供商", label: t("模型提供商", "Model providers"), description: t("连接模型服务并管理模型", "Connect model services and manage models"), icon: Bot },
        { id: "个性化", label: t("个性化", "Personalization"), description: t("定义 Yode 的偏好与工作方式", "Shape Yode's preferences and working style"), icon: Sparkles },
        { id: "键盘快捷键", label: t("键盘快捷键", "Keyboard shortcuts"), description: t("查看并自定义高频操作", "View and customize frequent actions"), icon: Command }
      ]
    },
    {
      title: t("应用集成", "Integrations"),
      items: [
        { id: "应用截图", label: t("应用截图", "Appshots"), description: t("管理应用截图与视觉上下文", "Manage app captures and visual context"), icon: MonitorPlay, comingSoon: true },
        { id: "MCP 服务器", label: t("MCP 服务器", "MCP servers"), description: t("连接外部工具与数据源", "Connect external tools and data sources"), icon: TerminalSquare },
        { id: "浏览器", label: t("浏览器", "Browser"), description: t("配置浏览器控制与隔离", "Configure browser control and isolation"), icon: Globe },
        { id: "计算机使用", label: t("计算机使用", "Computer use"), description: t("管理桌面交互与安全边界", "Manage desktop interaction and safety boundaries"), icon: Fingerprint }
      ]
    },
    {
      title: t("编码设置", "Coding"),
      items: [
        { id: "钩子", label: t("钩子", "Hooks"), description: t("在工具调用前后运行自动化", "Run automation around tool calls"), icon: GitBranch },
        { id: "连接", label: t("连接", "Connections"), description: t("管理开发服务连接", "Manage development service connections"), icon: Workflow, comingSoon: true },
        { id: "Git", label: t("Git", "Git"), description: t("配置提交、分支与仓库行为", "Configure commits, branches, and repository behavior"), icon: GitBranch },
        { id: "环境", label: t("环境", "Environments"), description: t("管理运行环境和依赖", "Manage runtimes and dependencies"), icon: Code2 },
        { id: "工作树", label: t("工作树", "Worktrees"), description: t("查看和清理 Git 工作树", "Inspect and clean up Git worktrees"), icon: Folder }
      ]
    },
    {
      title: t("已归档", "Archived"),
      items: [
        { id: "已归档对话", label: t("已归档对话", "Archived chats"), description: t("恢复或永久删除旧对话", "Restore or permanently remove old chats"), icon: Archive }
      ]
    },
    {
      title: t("其他", "Other"),
      items: [
        { id: "更新", label: t("更新", "Updates"), description: t("版本信息、更新与开源许可", "Version, updates, and open-source licenses"), icon: Download }
      ]
    }
  ];

  const normalizedSearch = searchQuery.trim().toLocaleLowerCase();
  const filteredCategories = categories
    .map((category) => ({
      ...category,
      items: category.items.filter((item) => {
        if (!normalizedSearch) return true;
        return [category.title, item.label, item.description]
          .join(" ")
          .toLocaleLowerCase()
          .includes(normalizedSearch);
      })
    }))
    .filter((category) => category.items.length > 0);
  const activeItem = categories
    .flatMap((category) => category.items)
    .find((item) => item.id === activeTab);

  useEffect(() => {
    const root = settingsRootRef.current;
    if (!root) return;
    let activeDialog: HTMLElement | null = null;
    let focusFrame = 0;

    const syncDialogFocus = () => {
      const nextDialog = root.querySelector<HTMLElement>(MANAGED_DIALOG_SELECTOR);
      if (nextDialog && nextDialog !== activeDialog) {
        activeDialog = nextDialog;
        const activeElement = document.activeElement instanceof HTMLElement
          && document.activeElement !== document.body
          && root.contains(document.activeElement)
          ? document.activeElement
          : null;
        dialogTriggerRef.current = activeElement ?? lastStableFocusRef.current;
        window.cancelAnimationFrame(focusFrame);
        focusFrame = window.requestAnimationFrame(() => {
          const initial = nextDialog.querySelector<HTMLElement>("[data-dialog-initial-focus]")
            ?? nextDialog.querySelector<HTMLElement>(
              "input:not([disabled]), textarea:not([disabled]), select:not([disabled])"
            )
            ?? nextDialog.querySelector<HTMLElement>("button:not([disabled])");
          initial?.focus();
        });
      } else if (!nextDialog && activeDialog) {
        activeDialog = null;
        window.cancelAnimationFrame(focusFrame);
        focusFrame = window.requestAnimationFrame(() => {
          const target = dialogTriggerRef.current?.isConnected
            ? dialogTriggerRef.current
            : lastStableFocusRef.current;
          target?.focus();
        });
      }
    };

    const observer = new MutationObserver(syncDialogFocus);
    observer.observe(root, { childList: true, subtree: true });
    syncDialogFocus();
    return () => {
      observer.disconnect();
      window.cancelAnimationFrame(focusFrame);
    };
  }, []);

  useEffect(() => {
    const handleEscape = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const dialog = settingsRootRef.current?.querySelector<HTMLElement>(MANAGED_DIALOG_SELECTOR);
      if (dialog) {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          dialog.querySelector<HTMLButtonElement>("[data-dialog-close]")?.click();
          return;
        }
        if (event.key === "Tab") {
          const controls = Array.from(dialog.querySelectorAll<HTMLElement>(MANAGED_DIALOG_FOCUSABLE))
            .filter((control) => control.getClientRects().length > 0);
          if (controls.length === 0) {
            event.preventDefault();
            dialog.focus();
            return;
          }
          const currentIndex = controls.indexOf(document.activeElement as HTMLElement);
          const nextIndex = event.shiftKey
            ? (currentIndex <= 0 ? controls.length - 1 : currentIndex - 1)
            : (currentIndex === controls.length - 1 ? 0 : currentIndex + 1);
          event.preventDefault();
          controls[nextIndex]?.focus();
          return;
        }
      }
      if (event.key !== "Escape") return;
      if (document.querySelector('[role="dialog"], [role="listbox"]')) return;
      event.preventDefault();
      if (searchQuery) {
        setSearchQuery("");
        return;
      }
      onClose();
    };
    window.addEventListener("keydown", handleEscape);
    return () => window.removeEventListener("keydown", handleEscape);
  }, [onClose, searchQuery]);

  return (
    <div
      ref={settingsRootRef}
      className={`settings-layout ${draggingSidebar ? "settings-sidebar-dragging" : ""}`}
      style={{ "--settings-sidebar-width": `${sidebarWidth}px` } as React.CSSProperties}
      onFocusCapture={(event) => {
        const target = event.target instanceof HTMLElement ? event.target : null;
        if (!target || target.closest(MANAGED_DIALOG_SELECTOR)) return;
        const menu = target.closest<HTMLElement>('[role="menu"][id]');
        const controller = menu
          ? Array.from(settingsRootRef.current?.querySelectorAll<HTMLElement>("[aria-controls]") ?? [])
            .find((candidate) => candidate.getAttribute("aria-controls") === menu.id)
          : null;
        lastStableFocusRef.current = controller ?? target;
      }}
    >
      <aside id="settings-navigation" className="settings-tabs settings-navigation" aria-label={t("设置导航", "Settings navigation")}>
        <button
          className="settings-tab back-tab-btn"
          onClick={onClose}
          type="button"
          aria-label={t("返回对话", "Back to app")}
        >
          <ArrowLeft size={15} />
          {t("返回对话", "Back to app")}
        </button>

        <div className="settings-search-field">
          <Search size={14} aria-hidden="true" />
          <input
            type="text"
            placeholder={t("搜索设置…", "Search settings…")}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            aria-label={t("搜索设置", "Search settings")}
          />
          {searchQuery ? (
            <button
              type="button"
              className="settings-search-clear"
              onClick={() => setSearchQuery("")}
              aria-label={t("清空搜索", "Clear search")}
              title={t("清空搜索", "Clear search")}
            >
              <X size={12} />
            </button>
          ) : null}
        </div>

        <div className="settings-navigation-scroll">
          {filteredCategories.map((category) => (
              <div className="settings-category" key={category.title}>
                <div className="settings-category-title">
                  {category.title}
                </div>
                {category.items.map((item) => {
                  const Icon = item.icon;
                  const isActive = activeTab === item.id;
                  return (
                    <button
                      className={`settings-tab ${isActive ? "active" : ""}`}
                      key={item.id}
                      onClick={() => handleSetActiveTab(item.id)}
                      type="button"
                      aria-current={isActive ? "page" : undefined}
                      title={item.description}
                    >
                      <Icon size={14} className="tab-icon" />
                      <span className="settings-tab-label">
                        {item.label}
                      </span>
                      {item.comingSoon ? (
                        <span className="settings-tab-badge">{t("即将推出", "Soon")}</span>
                      ) : null}
                    </button>
                  );
                })}
              </div>
          ))}
          {filteredCategories.length === 0 ? (
            <div className="settings-search-empty" role="status">
              <Search size={18} />
              <strong>{t("没有匹配的设置", "No matching settings")}</strong>
              <span>{t("试试功能名称或更短的关键词", "Try a feature name or shorter keyword")}</span>
            </div>
          ) : null}
        </div>
      </aside>
      <div
        className="settings-sidebar-resizer"
        onPointerDown={beginSidebarDrag}
        onKeyDown={resizeSettingsSidebarWithKeyboard}
        onDoubleClick={() => setSettingsSidebarWidth(224)}
        role="separator"
        aria-orientation="vertical"
        aria-controls="settings-navigation"
        aria-valuemin={180}
        aria-valuemax={340}
        aria-valuenow={Math.round(sidebarWidth)}
        tabIndex={0}
        title={t("拖动或使用方向键调整设置侧栏宽度；双击恢复默认", "Drag or use arrow keys to resize the settings sidebar; double-click to reset")}
      />
      <section className="settings-content" aria-labelledby="settings-page-title">
        <SettingsFileWarning
          status={settingsFileStatus ?? { loaded: true, path: "" }}
          isZh={isZh}
          onOpenFile={handleOpenSettingsFile}
          onRetry={() => void refreshSettingsFileStatus()}
          onRestore={() => void handleRestoreSettingsFile()}
        />
        <div className="settings-container">
          <div className="settings-heading settings-page-heading">
            <div className="settings-heading-copy">
              <div className="settings-heading-title-row">
                <h1 id="settings-page-title">{activeItem?.label ?? activeTab}</h1>
                {activeItem?.comingSoon ? (
                  <span className="settings-page-badge">{t("即将推出", "Coming soon")}</span>
                ) : null}
              </div>
              {activeItem?.description ? <p>{activeItem.description}</p> : null}
            </div>
          </div>

          {activeTab === "常规" && (
            <GeneralSettings bootstrap={bootstrap} isZh={isZh} t={t} />
          )}

          {activeTab === "外观" && (
            <AppearanceSettings />
          )}

          {activeTab === "配置" && (
            <ConfigurationSettings bootstrap={bootstrap} isZh={isZh} t={t} />
          )}

          {activeTab === "个性化" && (
            <PersonalizationSettings isZh={isZh} t={t} />
          )}

          {activeTab === "键盘快捷键" && (
            <KeyboardShortcutsSettings isZh={isZh} t={t} />
          )}

          {activeTab === "MCP 服务器" && (
            <McpSettingsSettings isZh={isZh} t={t} />
          )}

          {activeTab === "浏览器" && (
            <BrowserSettingsSettings isZh={isZh} t={t} />
          )}

          {activeTab === "计算机使用" && (
            <ComputerUseSettingsSettings isZh={isZh} t={t} />
          )}

          {activeTab === "钩子" && (
            <HooksSettingsSettings isZh={isZh} t={t} />
          )}

          {activeTab === "Git" && (
            <GitSettingsSettings isZh={isZh} t={t} />
          )}

          {activeTab === "环境" && (
            <EnvironmentsSettingsSettings isZh={isZh} t={t} />
          )}

          {activeTab === "工作树" && (
            <WorktreesSettingsSettings isZh={isZh} t={t} />
          )}

          {activeTab === "已归档对话" && (
            <ArchivedChatsSettingsSettings isZh={isZh} t={t} />
          )}

          {activeTab === "模型提供商" && (
            <ProvidersSettings bootstrap={bootstrap} isZh={isZh} t={t} />
          )}

          {activeTab === "更新" && (
            <AboutSettings bootstrap={bootstrap} isZh={isZh} t={t} />
          )}

          {activeTab !== "常规" && activeTab !== "外观" && activeTab !== "配置" && activeTab !== "模型提供商" && activeTab !== "个性化" && activeTab !== "键盘快捷键" && activeTab !== "MCP 服务器" && activeTab !== "浏览器" && activeTab !== "计算机使用" && activeTab !== "钩子" && activeTab !== "Git" && activeTab !== "环境" && activeTab !== "工作树" && activeTab !== "已归档对话" && activeTab !== "更新" && (
            <div className="settings-group compact">
              <div className="empty-state">
                <Bot size={20} />
                <span>{t(`${activeItem?.label ?? activeTab} 模块正在打磨中`, `${activeItem?.label ?? activeTab} is being polished`)}</span>
              </div>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
