import React, { useRef, useState, useEffect } from "react";
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
  Search
} from "lucide-react";
import { Bootstrap } from "../lib/desktopTypes";

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

  const handleSetActiveTab = (tab: string) => {
    setActiveTab(saveActiveSettingsTab(tab));
  };

  const [currentLang, setCurrentLang] = useState(() => loadAppLanguage());
  const isZh = currentLang === "zh";

  const t = (zhText: string, enText: string) => {
    return isZh ? zhText : enText;
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

  const categories = [
    {
      title: t("个人设置", "Personal"),
      items: [
        { id: "常规", label: t("常规", "General"), icon: Settings },
        { id: "外观", label: t("外观", "Appearance"), icon: Eye },
        { id: "配置", label: t("配置", "Configuration"), icon: Sliders },
        { id: "模型提供商", label: t("模型提供商", "Model providers"), icon: Bot },
        { id: "个性化", label: t("个性化", "Personalization"), icon: Sparkles },
        { id: "键盘快捷键", label: t("键盘快捷键", "Keyboard shortcuts"), icon: Command }
      ]
    },
    {
      title: t("应用集成", "Integrations"),
      items: [
        { id: "应用截图", label: t("应用截图", "Appshots"), icon: MonitorPlay },
        { id: "MCP 服务器", label: t("MCP 服务器", "MCP servers"), icon: TerminalSquare },
        { id: "浏览器", label: t("浏览器", "Browser"), icon: Globe },
        { id: "计算机使用", label: t("计算机使用", "Computer use"), icon: Fingerprint }
      ]
    },
    {
      title: t("编码设置", "Coding"),
      items: [
        { id: "钩子", label: t("钩子", "Hooks"), icon: GitBranch },
        { id: "连接", label: t("连接", "Connections"), icon: Workflow },
        { id: "Git", label: t("Git", "Git"), icon: GitBranch },
        { id: "环境", label: t("环境", "Environments"), icon: Code2 },
        { id: "工作树", label: t("工作树", "Worktrees"), icon: Folder }
      ]
    },
    {
      title: t("已归档", "Archived"),
      items: [
        { id: "已归档对话", label: t("已归档对话", "Archived chats"), icon: Archive }
      ]
    },
    {
      title: t("其他", "Other"),
      items: [
        { id: "更新", label: t("更新", "Updates"), icon: Download }
      ]
    }
  ];

  return (
    <div
      className={`settings-layout ${draggingSidebar ? "settings-sidebar-dragging" : ""}`}
      style={{ "--settings-sidebar-width": `${sidebarWidth}px` } as React.CSSProperties}
    >
      <aside className="settings-tabs" style={{ paddingTop: "32px", paddingInline: "12px", gap: "14px" }}>
        {/* Back Button */}
        <button
          className="settings-tab back-tab-btn"
          onClick={onClose}
          type="button"
          style={{
            border: "none",
            borderRadius: "var(--radius)",
            fontWeight: "600",
            fontSize: "13px",
            color: "var(--text-soft)",
            display: "flex",
            alignItems: "center",
            gap: "8px",
            background: "transparent",
            paddingInline: "8px",
            paddingBlock: "4px",
            cursor: "pointer",
            width: "100%",
            textAlign: "left",
            marginBottom: "4px"
          }}
        >
          <ArrowLeft size={15} />
          {t("返回对话", "Back to app")}
        </button>

        {/* Search settings bar */}
        <div style={{ position: "relative", width: "100%" }}>
          <Search size={13} style={{ position: "absolute", left: "9px", top: "7px", color: "var(--text-soft)", opacity: 0.8 }} />
          <input
            type="text"
            placeholder={t("搜索设置...", "Search settings...")}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            style={{
              width: "100%",
              height: "26px",
              background: "var(--field)",
              border: "none",
              borderRadius: "var(--radius)",
              paddingLeft: "26px",
              paddingRight: "8px",
              fontSize: "11.5px",
              color: "var(--text)",
              outline: "none"
            }}
          />
        </div>

        {/* Categorized menu items */}
        <div style={{ display: "flex", flexDirection: "column", gap: "12px", overflowY: "auto", flex: 1, paddingRight: "2px" }}>
          {categories.map((category) => {
            const filteredItems = category.items.filter((item) =>
              item.label.toLowerCase().includes(searchQuery.toLowerCase())
            );

            if (filteredItems.length === 0) return null;

            return (
              <div key={category.title} style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
                <div style={{
                  fontSize: "10.5px",
                  fontWeight: "700",
                  color: "var(--text-soft)",
                  opacity: 0.6,
                  paddingLeft: "10px",
                  textTransform: "capitalize",
                  marginBottom: "1px",
                  letterSpacing: "0.3px"
                }}>
                  {category.title}
                </div>
                {filteredItems.map((item) => {
                  const Icon = item.icon;
                  const isActive = activeTab === item.id;
                  return (
                    <button
                      className={`settings-tab ${isActive ? "active" : ""}`}
                      key={item.id}
                      onClick={() => handleSetActiveTab(item.id)}
                      type="button"
                      style={{
                        paddingBlock: "5px",
                        paddingInline: "10px",
                        fontSize: "12.5px",
                        fontWeight: isActive ? "600" : "500",
                        borderRadius: "var(--radius)",
                        display: "flex",
                        alignItems: "center",
                        gap: "8px",
                        width: "100%",
                        textAlign: "left",
                        background: isActive ? "color-mix(in oklch, var(--accent-muted), transparent 42%)" : "transparent",
                        color: isActive ? "var(--text)" : "color-mix(in oklch, var(--text-muted), transparent 20%)",
                        border: "none",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap"
                      }}
                    >
                      <Icon size={13} className="tab-icon" style={{ flexShrink: 0, color: isActive ? "var(--accent)" : "var(--text-soft)" }} />
                      <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", flex: 1 }}>
                        {item.label}
                      </span>
                    </button>
                  );
                })}
              </div>
            );
          })}
        </div>
      </aside>
      <div
        className="settings-sidebar-resizer"
        onPointerDown={beginSidebarDrag}
        role="separator"
        aria-orientation="vertical"
        title={t("拖动调整设置侧边栏宽度", "Drag to resize settings sidebar")}
      />
      <section className="settings-content" style={{ display: "flex", flexDirection: "column", alignItems: "center" }}>
        <div style={{ width: "100%", maxWidth: "720px" }}>
          <div className="settings-heading" style={{ marginBottom: "24px", paddingTop: "8px" }}>
            <div>
              <h1 style={{ margin: 0, fontSize: "22px", fontWeight: "600", letterSpacing: "-0.2px", color: "var(--text)" }}>{activeTab}</h1>
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
                <span>{activeTab} 模块的设置面板将在后续批次中接入</span>
              </div>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
