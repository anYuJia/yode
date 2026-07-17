import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X } from "lucide-react";
import { Bootstrap, SessionSummary } from "../../lib/desktopTypes";
import { CustomSelect } from "../CustomSelect";
import {
  LANGUAGE_CHANGE_EVENT,
  languageFromChangeEvent,
  loadAppLanguage,
  saveAppLanguage
} from "../../lib/appearanceSettings";
import {
  applyGeneralSettings,
  loadGeneralSettingsPayload,
  saveGeneralSettingValue
} from "../../lib/desktopSettings";
import { dispatchSessionsImported } from "../../lib/projectStorage";

type LicenseNotice = {
  name: string;
  version?: string | null;
  license?: string | null;
  source: string;
};

type ImportAiSessionsResult = {
  imported: number;
  skipped: number;
  sessions: SessionSummary[];
};

export function GeneralSettings({
  bootstrap,
  t
}: {
  bootstrap: Bootstrap;
  isZh?: boolean;
  t: (zh: string, en: string) => string;
}) {
  const initialGeneralSettings = loadGeneralSettingsPayload();
  const [workMode, setWorkMode] = useState(initialGeneralSettings.workMode);
  const [defPerm, setDefPerm] = useState(initialGeneralSettings.defaultFilePermission);
  const [autoReview, setAutoReview] = useState(initialGeneralSettings.autoReview);
  const [fullAccess, setFullAccess] = useState(initialGeneralSettings.fullAccess);
  const [openDest, setOpenDest] = useState(initialGeneralSettings.openDestination);
  const [showInMenuBar, setShowInMenuBar] = useState(initialGeneralSettings.showInMenuBar);
  const [bottomPanel, setBottomPanel] = useState(initialGeneralSettings.bottomPanel);
  const [termLoc, setTermLoc] = useState(initialGeneralSettings.terminalLocation);
  const [preventSleep, setPreventSleep] = useState(initialGeneralSettings.preventSleep);
  const [codeReviewPolicy, setCodeReviewPolicy] = useState(initialGeneralSettings.codeReviewPolicy);
  const [suggestedPrompts, setSuggestedPrompts] = useState(initialGeneralSettings.suggestedPrompts);
  const [contextUsage, setContextUsage] = useState(initialGeneralSettings.contextUsage);
  const [followUpBehavior, setFollowUpBehavior] = useState(initialGeneralSettings.followUpBehavior);
  const [requireOptEnter, setRequireOptEnter] = useState(initialGeneralSettings.requireOptEnter);
  const [completionNotif, setCompletionNotif] = useState(initialGeneralSettings.completionNotification);
  const [permNotif, setPermNotif] = useState(initialGeneralSettings.permissionNotification);
  const [questionNotif, setQuestionNotif] = useState(initialGeneralSettings.questionNotification);
  const [currentLang, setCurrentLang] = useState(() => loadAppLanguage());
  const [licenseModalOpen, setLicenseModalOpen] = useState(false);
  const [licenseNotices, setLicenseNotices] = useState<LicenseNotice[]>([]);
  const [licenseLoading, setLicenseLoading] = useState(false);
  const [importStatus, setImportStatus] = useState("");

  const updateGeneralVal = (key: string, value: string | boolean) => {
    saveGeneralSettingValue(key, value);
    void applyGeneralSettings();
  };

  useEffect(() => {
    void applyGeneralSettings();
  }, []);

  useEffect(() => {
    const handleLangChange = (e: Event) => {
      setCurrentLang(languageFromChangeEvent(e));
    };
    window.addEventListener(LANGUAGE_CHANGE_EVENT, handleLangChange);
    return () => window.removeEventListener(LANGUAGE_CHANGE_EVENT, handleLangChange);
  }, []);

  const handleOpenCurrentProject = () => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    invoke("open_target", { request: { target: openDest, path: bootstrap.workspacePath } }).catch((err) => {
      console.error(err);
      setImportStatus(t("打开目标失败，请确认对应应用已安装。", "Failed to open target. Check that the app is installed."));
    });
  };

  const handleImportAiSessions = async () => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    setImportStatus(t("正在导入...", "Importing..."));
    try {
      const result = await invoke<ImportAiSessionsResult>("import_ai_sessions");
      setImportStatus(t("已导入 " + result.imported + " 个会话，跳过 " + result.skipped + " 个文件。", "Imported " + result.imported + " sessions, skipped " + result.skipped + " files."));
      if (result.imported > 0) {
        dispatchSessionsImported(result.sessions);
      }
    } catch (err) {
      console.error(err);
      setImportStatus(t("导入失败，请检查文件格式。", "Import failed. Check the file format."));
    }
  };

  const handleOpenLicenses = async () => {
    setLicenseModalOpen(true);
    if (!("__TAURI_INTERNALS__" in window)) return;
    setLicenseLoading(true);
    try {
      setLicenseNotices(await invoke<LicenseNotice[]>("license_notices"));
    } catch (err) {
      console.error(err);
      setLicenseNotices([]);
    } finally {
      setLicenseLoading(false);
    }
  };

  return (
    <>
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
                  <button
                    className="secondary-button"
                    style={{ paddingInline: "12px", height: "28px", marginLeft: "8px" }}
                    type="button"
                    onClick={handleOpenCurrentProject}
                  >
                    {t("打开", "Open")}
                  </button>
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
                      setCurrentLang(saveAppLanguage(val));
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
                  <button
                    className="secondary-button"
                    style={{ paddingInline: "14px", height: "28px" }}
                    type="button"
                    onClick={handleImportAiSessions}
                  >
                    {t("导入", "Import")}
                  </button>
                </div>
                {importStatus && (
                  <div style={{ fontSize: "11px", color: "var(--text-soft)", padding: "2px 0 0 2px" }}>
                    {importStatus}
                  </div>
                )}
                <div className="divider" />

                <div className="form-row">
                  <div className="row-info">
                    <span className="row-label">{t("开源许可声明", "Open source licenses")}</span>
                    <span className="row-desc">{t("查看所包含依赖项的第三方声明", "Third-party notices for bundled dependencies")}</span>
                  </div>
                  <button
                    className="secondary-button"
                    style={{ paddingInline: "14px", height: "28px" }}
                    type="button"
                    onClick={handleOpenLicenses}
                  >
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
      {licenseModalOpen && (
        <div className="settings-modal-backdrop" onClick={() => setLicenseModalOpen(false)}>
          <div className="settings-modal" onClick={(event) => event.stopPropagation()}>
            <div className="settings-modal-header">
              <div>
                <h2>{t("开源许可声明", "Open source licenses")}</h2>
                <p>{t("当前桌面端包含的 Rust 与前端依赖清单", "Bundled Rust and frontend dependency notices")}</p>
              </div>
              <button type="button" className="icon-button" onClick={() => setLicenseModalOpen(false)} aria-label={t("关闭", "Close")}>
                <X size={16} />
              </button>
            </div>
            <div className="license-list">
              {licenseLoading ? (
                <div className="empty-state">{t("正在读取许可声明...", "Loading notices...")}</div>
              ) : licenseNotices.length === 0 ? (
                <div className="empty-state">{t("没有读取到依赖声明。", "No dependency notices found.")}</div>
              ) : (
                licenseNotices.map((notice) => (
                  <div className="license-row" key={notice.source + "-" + notice.name + "-" + (notice.version || "")}>
                    <div>
                      <strong>{notice.name}</strong>
                      {notice.version && <span>{notice.version}</span>}
                    </div>
                    <em>{notice.license || t("许可信息需查看上游包声明", "See upstream package notice")}</em>
                    <small>{notice.source}</small>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}
    </>
  );
}
