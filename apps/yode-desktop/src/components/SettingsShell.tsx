import React, { useState, useEffect } from "react";
import {
  SlidersHorizontal,
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
  History,
  KeyRound,
  Bot,
  Sun,
  Moon,
  Monitor,
  Copy,
  Download,
  ChevronDown,
  ArrowLeft,
  Search,
  X,
  Plus,
  Trash2
} from "lucide-react";
import { Bootstrap } from "../lib/mock";
import { CustomSelect, CustomSelectOption } from "./CustomSelect";
import { ColorPicker } from "./ColorPicker";

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

export function SettingsShell({ bootstrap, onClose }: { bootstrap: Bootstrap; onClose: () => void }) {
  const [activeTab, setActiveTab] = useState(() => localStorage.getItem("yode-active-tab") || "常规");
  const [searchQuery, setSearchQuery] = useState("");

  const handleSetActiveTab = (tab: string) => {
    setActiveTab(tab);
    localStorage.setItem("yode-active-tab", tab);
  };

  const [currentLang, setCurrentLang] = useState(() => localStorage.getItem("yode-language") || "zh");
  const isZh = currentLang === "zh";

  const t = (zhText: string, enText: string) => {
    return isZh ? zhText : enText;
  };

  useEffect(() => {
    const handleLangChange = (e: Event) => {
      const newLang = (e as CustomEvent).detail;
      setCurrentLang(newLang);
    };
    window.addEventListener("yode-language-change", handleLangChange);
    return () => window.removeEventListener("yode-language-change", handleLangChange);
  }, []);

  // State bindings for General configuration elements
  const [workMode, setWorkMode] = useState(() => localStorage.getItem("yode-work-mode") || "coding");
  const [defPerm, setDefPerm] = useState(() => localStorage.getItem("yode-def-perm") !== "false");
  const [autoReview, setAutoReview] = useState(() => localStorage.getItem("yode-auto-review") !== "false");
  const [fullAccess, setFullAccess] = useState(() => localStorage.getItem("yode-full-access") !== "false");
  const [openDest, setOpenDest] = useState(() => localStorage.getItem("yode-open-dest") || "VS Code");
  const [showInMenuBar, setShowInMenuBar] = useState(() => localStorage.getItem("yode-show-menu-bar") !== "false");
  const [bottomPanel, setBottomPanel] = useState(() => localStorage.getItem("yode-bottom-panel") !== "false");
  const [termLoc, setTermLoc] = useState(() => localStorage.getItem("yode-term-loc") || "bottom");
  const [preventSleep, setPreventSleep] = useState(() => localStorage.getItem("yode-prevent-sleep") === "true");
  const [codeReviewPolicy, setCodeReviewPolicy] = useState(() => localStorage.getItem("yode-code-review-policy") || "inline");
  const [suggestedPrompts, setSuggestedPrompts] = useState(() => localStorage.getItem("yode-suggested-prompts") !== "false");
  const [contextUsage, setContextUsage] = useState(() => localStorage.getItem("yode-context-usage") === "true");
  const [followUpBehavior, setFollowUpBehavior] = useState(() => localStorage.getItem("yode-follow-up-behavior") || "queue");
  const [requireOptEnter, setRequireOptEnter] = useState(() => localStorage.getItem("yode-require-opt-enter") === "true");
  const [completionNotif, setCompletionNotif] = useState(() => localStorage.getItem("yode-completion-notif") || "Only when unfocused");
  const [permNotif, setPermNotif] = useState(() => localStorage.getItem("yode-perm-notif") !== "false");
  const [questionNotif, setQuestionNotif] = useState(() => localStorage.getItem("yode-question-notif") !== "false");

  const updateGeneralVal = (key: string, value: string | boolean) => {
    localStorage.setItem(key, String(value));
  };

  const categories = [
    {
      title: t("个人设置", "Personal"),
      items: [
        { id: "常规", label: t("常规", "General"), icon: Settings },
        { id: "外观", label: t("外观", "Appearance"), icon: Eye },
        { id: "配置", label: t("配置", "Configuration"), icon: Sliders },
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
    }
  ];

  return (
    <div className="settings-layout">
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
      <section className="settings-content" style={{ display: "flex", flexDirection: "column", alignItems: "center" }}>
        <div style={{ width: "100%", maxWidth: "720px" }}>
          <div className="settings-heading" style={{ marginBottom: "24px", paddingTop: "8px" }}>
            <div>
              <h1 style={{ margin: 0, fontSize: "22px", fontWeight: "600", letterSpacing: "-0.2px", color: "var(--text)" }}>{activeTab}</h1>
            </div>
          </div>

          {activeTab === "常规" && (
            <div className="appearance-container">
              {/* 1. Work Mode Section */}
              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
                  {t("工作模式", "Work mode")}
                </span>
                <span style={{ fontSize: "11px", color: "var(--text-soft)", marginBottom: "4px" }}>
                  {t("选择 Yode 展示技术细节的深度", "Choose how much technical detail Yode shows")}
                </span>
                <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "12px" }}>
                  <button
                    type="button"
                    onClick={() => {
                      setWorkMode("coding");
                      updateGeneralVal("yode-work-mode", "coding");
                    }}
                    style={{
                      padding: "10px 14px",
                      borderRadius: "var(--radius)",
                      background: "var(--field)",
                      border: "1px solid var(--line-soft)",
                      textAlign: "left",
                      cursor: "pointer",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between"
                    }}
                  >
                    <div>
                      <div style={{ fontSize: "12.5px", fontWeight: "600", color: "var(--text)" }}>{t("专注编码", "For coding")}</div>
                      <div style={{ fontSize: "11px", color: "var(--text-soft)", marginTop: "2px" }}>{t("提供更多技术性响应与工具掌控", "More technical responses and control")}</div>
                    </div>
                    <div style={{
                      width: "16px",
                      height: "16px",
                      borderRadius: "50%",
                      border: `2px solid ${workMode === "coding" ? "var(--accent)" : "var(--line)"}`,
                      background: workMode === "coding" ? "var(--accent)" : "transparent",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center"
                    }}>
                      {workMode === "coding" && <div style={{ width: "6px", height: "6px", borderRadius: "50%", background: "var(--bg)" }} />}
                    </div>
                  </button>

                  <button
                    type="button"
                    onClick={() => {
                      setWorkMode("everyday");
                      updateGeneralVal("yode-work-mode", "everyday");
                    }}
                    style={{
                      padding: "10px 14px",
                      borderRadius: "var(--radius)",
                      background: "var(--field)",
                      border: "1px solid var(--line-soft)",
                      textAlign: "left",
                      cursor: "pointer",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between"
                    }}
                  >
                    <div>
                      <div style={{ fontSize: "12.5px", fontWeight: "600", color: "var(--text)" }}>{t("日常工作", "For everyday work")}</div>
                      <div style={{ fontSize: "11px", color: "var(--text-soft)", marginTop: "2px" }}>{t("相同的智能，但减少复杂技术细节", "Same power, less technical detail")}</div>
                    </div>
                    <div style={{
                      width: "16px",
                      height: "16px",
                      borderRadius: "50%",
                      border: `2px solid ${workMode === "everyday" ? "var(--accent)" : "var(--line)"}`,
                      background: workMode === "everyday" ? "var(--accent)" : "transparent",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center"
                    }}>
                      {workMode === "everyday" && <div style={{ width: "6px", height: "6px", borderRadius: "50%", background: "var(--bg)" }} />}
                    </div>
                  </button>
                </div>
              </div>

              {/* 2. Permissions Card */}
              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
                  {t("权限控制", "Permissions")}
                </span>
                <div className="theme-card">
                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("默认文件权限", "Default permissions")}</span>
                      <span className="row-desc">{t("默认情况下，Yode 可以读取和编辑其工作区中的文件", "By default, Yode can read and edit files in its workspace")}</span>
                    </div>
                    <label className="switch-wrapper">
                      <input
                        type="checkbox"
                        checked={defPerm}
                        onChange={(e) => {
                          setDefPerm(e.target.checked);
                          updateGeneralVal("yode-def-perm", e.target.checked);
                        }}
                      />
                      <span className="switch-slider" />
                    </label>
                  </div>
                  <div className="divider" />
                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("自动代码审查", "Auto-review")}</span>
                      <span className="row-desc">{t("自动审查代码修改，以便发现潜在的设计隐患", "Yode automatically reviews requests for additional access")}</span>
                    </div>
                    <label className="switch-wrapper">
                      <input
                        type="checkbox"
                        checked={autoReview}
                        onChange={(e) => {
                          setAutoReview(e.target.checked);
                          updateGeneralVal("yode-auto-review", e.target.checked);
                        }}
                      />
                      <span className="switch-slider" />
                    </label>
                  </div>
                  <div className="divider" />
                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("完整系统访问权限", "Full access")}</span>
                      <span className="row-desc">{t("允许 Yode 编辑系统文件并执行本地终端指令", "Allows Yode to run shell commands and modify local files")}</span>
                    </div>
                    <label className="switch-wrapper">
                      <input
                        type="checkbox"
                        checked={fullAccess}
                        onChange={(e) => {
                          setFullAccess(e.target.checked);
                          updateGeneralVal("yode-full-access", e.target.checked);
                        }}
                      />
                      <span className="switch-slider" />
                    </label>
                  </div>
                </div>
              </div>

              {/* 3. General Config Form */}
              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
                  {t("通用", "General")}
                </span>
                <div className="theme-card">
                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("默认打开目标", "Default open destination")}</span>
                      <span className="row-desc">{t("默认情况下打开文件和文件夹的位置", "Where files and folders open by default")}</span>
                    </div>
                    <CustomSelect
                      value={openDest}
                      onChange={(val) => {
                        setOpenDest(val);
                        updateGeneralVal("yode-open-dest", val);
                      }}
                      options={[
                        { value: "VS Code", label: "VS Code", avatarText: "💻", avatarBg: "rgba(255,255,255,0.05)" },
                        { value: "Cursor", label: "Cursor", avatarText: "🤖", avatarBg: "rgba(255,255,255,0.05)" },
                        { value: "Terminal", label: "Terminal", avatarText: "🐚", avatarBg: "rgba(255,255,255,0.05)" }
                      ]}
                      style={{ minWidth: "160px" }}
                    />
                  </div>
                  <div className="divider" />

                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("界面语言", "Language")}</span>
                      <span className="row-desc">{t("设置 Yode 的界面显示语言", "Language for the app UI")}</span>
                    </div>
                    <CustomSelect
                      value={currentLang}
                      onChange={(val) => {
                        localStorage.setItem("yode-language", val);
                        setCurrentLang(val);
                        // Trigger a Custom Event that App.tsx listens to to update its local language state dynamically
                        window.dispatchEvent(new CustomEvent("yode-language-change", { detail: val }));
                      }}
                      options={[
                        { value: "zh", label: "简体中文 (Simplified Chinese)", avatarText: "🇨🇳", avatarBg: "rgba(255,255,255,0.05)" },
                        { value: "en", label: "English (US)", avatarText: "🇺🇸", avatarBg: "rgba(255,255,255,0.05)" }
                      ]}
                      style={{ minWidth: "200px" }}
                    />
                  </div>
                  <div className="divider" />

                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("在菜单栏中显示", "Show in menu bar")}</span>
                      <span className="row-desc">{t("主窗口关闭时将 Yode 保留在系统状态栏中", "Keep Yode in the macOS menu bar when the main window is closed")}</span>
                    </div>
                    <label className="switch-wrapper">
                      <input
                        type="checkbox"
                        checked={showInMenuBar}
                        onChange={(e) => {
                          setShowInMenuBar(e.target.checked);
                          updateGeneralVal("yode-show-menu-bar", e.target.checked);
                        }}
                      />
                      <span className="switch-slider" />
                    </label>
                  </div>
                  <div className="divider" />

                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("底部控制面板", "Bottom panel")}</span>
                      <span className="row-desc">{t("在应用底部状态栏显示核心操控面板", "Show the bottom panel control in the app header")}</span>
                    </div>
                    <label className="switch-wrapper">
                      <input
                        type="checkbox"
                        checked={bottomPanel}
                        onChange={(e) => {
                          setBottomPanel(e.target.checked);
                          updateGeneralVal("yode-bottom-panel", e.target.checked);
                        }}
                      />
                      <span className="switch-slider" />
                    </label>
                  </div>
                  <div className="divider" />

                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("默认终端位置", "Default terminal location")}</span>
                      <span className="row-desc">{t("选择终端面板和环境动作在何处展开", "Choose where the terminal shortcut and environment actions open")}</span>
                    </div>
                    <div className="segmented-control">
                      <button
                        className={`segmented-btn ${termLoc === "bottom" ? "active" : ""}`}
                        onClick={() => {
                          setTermLoc("bottom");
                          updateGeneralVal("yode-term-loc", "bottom");
                        }}
                        type="button"
                      >
                        {t("底部", "Bottom")}
                      </button>
                      <button
                        className={`segmented-btn ${termLoc === "right" ? "active" : ""}`}
                        onClick={() => {
                          setTermLoc("right");
                          updateGeneralVal("yode-term-loc", "right");
                        }}
                        type="button"
                      >
                        {t("右侧", "Right")}
                      </button>
                    </div>
                  </div>
                  <div className="divider" />

                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("运行期间阻止休眠", "Prevent sleep while running")}</span>
                      <span className="row-desc">{t("当 Yode 执行任务时保持计算机处于唤醒状态", "Keep your computer awake while Yode is running a chat")}</span>
                    </div>
                    <label className="switch-wrapper">
                      <input
                        type="checkbox"
                        checked={preventSleep}
                        onChange={(e) => {
                          setPreventSleep(e.target.checked);
                          updateGeneralVal("yode-prevent-sleep", e.target.checked);
                        }}
                      />
                      <span className="switch-slider" />
                    </label>
                  </div>
                  <div className="divider" />

                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("代码审查策略", "Code review")}</span>
                      <span className="row-desc">{t("选择在原处进行对比还是在新窗口中展开", "Start /review in the current chat when possible or launch a separate review chat")}</span>
                    </div>
                    <div className="segmented-control">
                      <button
                        className={`segmented-btn ${codeReviewPolicy === "inline" ? "active" : ""}`}
                        onClick={() => {
                          setCodeReviewPolicy("inline");
                          updateGeneralVal("yode-code-review-policy", "inline");
                        }}
                        type="button"
                      >
                        {t("内联", "Inline")}
                      </button>
                      <button
                        className={`segmented-btn ${codeReviewPolicy === "detached" ? "active" : ""}`}
                        onClick={() => {
                          setCodeReviewPolicy("detached");
                          updateGeneralVal("yode-code-review-policy", "detached");
                        }}
                        type="button"
                      >
                        {t("独立对话", "Detached")}
                      </button>
                    </div>
                  </div>
                  <div className="divider" />

                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("智能提示建议", "Suggested prompts")}</span>
                      <span className="row-desc">{t("通过索引文件和已连接的应用提供相关指令提示", "Suggest what to do next by searching project files and connected apps")}</span>
                    </div>
                    <label className="switch-wrapper">
                      <input
                        type="checkbox"
                        checked={suggestedPrompts}
                        onChange={(e) => {
                          setSuggestedPrompts(e.target.checked);
                          updateGeneralVal("yode-suggested-prompts", e.target.checked);
                        }}
                      />
                      <span className="switch-slider" />
                    </label>
                  </div>
                  <div className="divider" />

                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("导入其他 AI 会话", "Import work from other AI apps")}</span>
                      <span className="row-desc">{t("将配置、项目及最近会话快速导入到 Yode", "Bring over your setup, projects, and recent chats")}</span>
                    </div>
                    <button className="secondary-button" style={{ paddingInline: "14px", height: "28px" }} type="button">
                      {t("导入", "Import")}
                    </button>
                  </div>
                  <div className="divider" />

                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("开源许可声明", "Open source licenses")}</span>
                      <span className="row-desc">{t("查看所包含依赖项的第三方声明", "Third-party notices for bundled dependencies")}</span>
                    </div>
                    <button className="secondary-button" style={{ paddingInline: "14px", height: "28px" }} type="button">
                      {t("查看", "View")}
                    </button>
                  </div>
                </div>
              </div>

              {/* 4. Composer Section */}
              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
                  {t("输入框设置", "Composer")}
                </span>
                <div className="theme-card">
                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("显示上下文窗口用量", "Show context window usage")}</span>
                    </div>
                    <label className="switch-wrapper">
                      <input
                        type="checkbox"
                        checked={contextUsage}
                        onChange={(e) => {
                          setContextUsage(e.target.checked);
                          updateGeneralVal("yode-context-usage", e.target.checked);
                        }}
                      />
                      <span className="switch-slider" />
                    </label>
                  </div>
                  <div className="divider" />
                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("追问行为控制", "Follow-up behavior")}</span>
                      <span className="row-desc">{t("连续追问时直接运行或等待确认", "Queue follow-ups while Yode runs or steer the current run")}</span>
                    </div>
                    <div className="segmented-control">
                      <button
                        className={`segmented-btn ${followUpBehavior === "queue" ? "active" : ""}`}
                        onClick={() => {
                          setFollowUpBehavior("queue");
                          updateGeneralVal("yode-follow-up-behavior", "queue");
                        }}
                        type="button"
                      >
                        {t("队列式", "Queue")}
                      </button>
                      <button
                        className={`segmented-btn ${followUpBehavior === "steer" ? "active" : ""}`}
                        onClick={() => {
                          setFollowUpBehavior("steer");
                          updateGeneralVal("yode-follow-up-behavior", "steer");
                        }}
                        type="button"
                      >
                        {t("指引式", "Steer")}
                      </button>
                    </div>
                  </div>
                  <div className="divider" />
                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("回车发送长指令", "Require ⌥ + enter to send long prompts")}</span>
                      <span className="row-desc">{t("开启后，多行输入框回车表示换行", "When enabled, multiline prompts require ⌥ + enter to send")}</span>
                    </div>
                    <label className="switch-wrapper">
                      <input
                        type="checkbox"
                        checked={requireOptEnter}
                        onChange={(e) => {
                          setRequireOptEnter(e.target.checked);
                          updateGeneralVal("yode-require-opt-enter", e.target.checked);
                        }}
                      />
                      <span className="switch-slider" />
                    </label>
                  </div>
                </div>
              </div>

              {/* 5. Notifications Section */}
              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
                  {t("通知机制", "Notifications")}
                </span>
                <div className="theme-card">
                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("任务完成通知", "Turn completion notifications")}</span>
                      <span className="row-desc">{t("设置当 Yode 任务执行完成时发送弹窗通知", "Set when Yode alerts you that it's finished")}</span>
                    </div>
                    <CustomSelect
                      value={completionNotif}
                      onChange={(val) => {
                        setCompletionNotif(val);
                        updateGeneralVal("yode-completion-notif", val);
                      }}
                      options={[
                        { value: "Only when unfocused", label: t("仅当失去焦点时", "Only when unfocused"), avatarText: "🔔" },
                        { value: "Always", label: t("总是通知", "Always"), avatarText: "🔊" },
                        { value: "Never", label: t("从不通知", "Never"), avatarText: "🔕" }
                      ]}
                      style={{ minWidth: "180px" }}
                    />
                  </div>
                  <div className="divider" />
                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("启用权限请求提示", "Enable permission notifications")}</span>
                      <span className="row-desc">{t("需要提权及敏感读写请求时发送通知", "Show alerts when notification permissions are required")}</span>
                    </div>
                    <label className="switch-wrapper">
                      <input
                        type="checkbox"
                        checked={permNotif}
                        onChange={(e) => {
                          setPermNotif(e.target.checked);
                          updateGeneralVal("yode-perm-notif", e.target.checked);
                        }}
                      />
                      <span className="switch-slider" />
                    </label>
                  </div>
                  <div className="divider" />
                  <div className="form-row">
                    <div className="row-info">
                      <span className="row-label">{t("启用追问输入提示", "Enable question notifications")}</span>
                      <span className="row-desc">{t("任务等待用户确认或者交互追问时发送通知", "Show alerts when input is needed to continue")}</span>
                    </div>
                    <label className="switch-wrapper">
                      <input
                        type="checkbox"
                        checked={questionNotif}
                        onChange={(e) => {
                          setQuestionNotif(e.target.checked);
                          updateGeneralVal("yode-question-notif", e.target.checked);
                        }}
                      />
                      <span className="switch-slider" />
                    </label>
                  </div>
                </div>
              </div>
            </div>
          )}

          {activeTab === "外观" && (
            <AppearanceSettings />
          )}

          {activeTab === "配置" && (
            <ConfigurationSettings isZh={isZh} t={t} />
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

          {activeTab !== "常规" && activeTab !== "外观" && activeTab !== "配置" && activeTab !== "个性化" && activeTab !== "键盘快捷键" && activeTab !== "MCP 服务器" && activeTab !== "浏览器" && activeTab !== "计算机使用" && activeTab !== "钩子" && activeTab !== "Git" && activeTab !== "环境" && activeTab !== "工作树" && activeTab !== "已归档对话" && (
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
