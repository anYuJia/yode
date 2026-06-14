import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, X, Settings, GitBranch } from "lucide-react";
import { isTauriRuntime, loadDesktopSetting } from "../../lib/desktopSettings";

interface HookEntry {
  name: string;
  events: string[];
  command: string;
  timeoutSecs: number;
  canBlock: boolean;
  disabled: boolean;
  toolFilter?: string[];
}

type HooksSettingsState = {
  enabled: boolean;
  hooks: HookEntry[];
};

const DEFAULT_HOOKS: HookEntry[] = [
  {
    name: "Pre-commit check",
    events: ["pre_turn"],
    command: "npm run lint",
    timeoutSecs: 15,
    canBlock: true,
    disabled: false
  },
  {
    name: "Auto-format code",
    events: ["task_completed"],
    command: "cargo fmt",
    timeoutSecs: 10,
    canBlock: false,
    disabled: false
  }
];

function normalizeHookEntry(raw: any): HookEntry | null {
  if (!raw || typeof raw !== "object") return null;
  const name = String(raw.name || "").trim();
  const command = String(raw.command || "").trim();
  const events = Array.isArray(raw.events) ? raw.events.map(String).filter(Boolean) : [];
  if (!name || !command || events.length === 0) return null;
  const toolFilterRaw = raw.toolFilter ?? raw.tool_filter;
  const toolFilter = Array.isArray(toolFilterRaw) ? toolFilterRaw.map(String).filter(Boolean) : undefined;
  return {
    name,
    command,
    events,
    timeoutSecs: Number(raw.timeoutSecs ?? raw.timeout_secs) || 10,
    canBlock: Boolean(raw.canBlock ?? raw.can_block),
    disabled: Boolean(raw.disabled),
    toolFilter: toolFilter && toolFilter.length > 0 ? toolFilter : undefined
  };
}

function normalizeHooks(list: unknown): HookEntry[] {
  if (!Array.isArray(list)) return [];
  return list.map(normalizeHookEntry).filter((hook): hook is HookEntry => hook !== null);
}

function persistHooksFallback(settings: HooksSettingsState) {
  localStorage.setItem("yode-hooks-enabled", JSON.stringify(settings.enabled));
  localStorage.setItem("yode-hooks-list", JSON.stringify(settings.hooks));
}

export function HooksSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [hooksEnabled, setHooksEnabled] = useState(() => {
    return localStorage.getItem("yode-hooks-enabled") !== "false";
  });
  const [statusText, setStatusText] = useState("");

  const [hooksList, setHooksList] = useState<HookEntry[]>(() => {
    const saved = localStorage.getItem("yode-hooks-list");
    if (saved) {
      try {
        return normalizeHooks(JSON.parse(saved));
      } catch (e) {
        // use defaults
      }
    }
    return DEFAULT_HOOKS;
  });

  useEffect(() => {
    const loadSettings = async () => {
      if (isTauriRuntime()) {
        try {
          const settings = await invoke<HooksSettingsState>("hooks_settings_get");
          applySettingsToState({ enabled: settings.enabled, hooks: normalizeHooks(settings.hooks) });
          setStatusText(t("钩子设置已连接到运行时。", "Hook settings are connected to the runtime."));
          return;
        } catch (err) {
          console.error(err);
        }
      }
      const enabled = await loadDesktopSetting("yode-hooks-enabled", hooksEnabled);
      const hooks = normalizeHooks(await loadDesktopSetting("yode-hooks-list", hooksList));
      applySettingsToState({ enabled, hooks: hooks.length > 0 ? hooks : DEFAULT_HOOKS });
    };
    void loadSettings();
  }, []);

  const currentSettings = (): HooksSettingsState => ({
    enabled: hooksEnabled,
    hooks: hooksList
  });

  const applySettingsToState = (settings: HooksSettingsState) => {
    setHooksEnabled(settings.enabled);
    setHooksList(normalizeHooks(settings.hooks));
  };

  const applyHooksSettings = async (nextSettings: HooksSettingsState) => {
    const normalized = { enabled: nextSettings.enabled, hooks: normalizeHooks(nextSettings.hooks) };
    try {
      if (isTauriRuntime()) {
        const applied = await invoke<HooksSettingsState>("hooks_settings_apply", { settings: normalized });
        applySettingsToState(applied);
      } else {
        persistHooksFallback(normalized);
        applySettingsToState(normalized);
      }
      setStatusText(t("钩子配置已保存。", "Hook configuration saved."));
    } catch (err) {
      console.error(err);
      setStatusText(t("保存钩子配置失败。", "Failed to save hook configuration."));
    }
  };

  const handleToggleHook = (index: number) => {
    const updated = [...hooksList];
    updated[index].disabled = !updated[index].disabled;
    void applyHooksSettings({ ...currentSettings(), hooks: updated });
  };

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [modalMode, setModalMode] = useState<"add" | "edit">("add");

  const [formName, setFormName] = useState("");
  const [formCommand, setFormCommand] = useState("");
  const [formEvents, setFormEvents] = useState<string[]>([]);
  const [formTimeout, setFormTimeout] = useState(10);
  const [formCanBlock, setFormCanBlock] = useState(false);
  const [formToolFilter, setFormToolFilter] = useState("");

  const eventOptions = [
    { value: "session_start", label: t("会话开始", "Session Start") },
    { value: "session_end", label: t("会话结束", "Session End") },
    { value: "pre_turn", label: t("回合前", "Pre-turn") },
    { value: "pre_compact", label: t("压缩前", "Pre-compaction") },
    { value: "post_compact", label: t("压缩后", "Post-compaction") },
    { value: "pre_tool_use", label: t("工具调用前", "Pre-tool Use") },
    { value: "post_tool_use", label: t("工具调用后", "Post-tool Use") },
    { value: "post_tool_use_failure", label: t("工具失败后", "Tool Failure") },
    { value: "subagent_start", label: t("子智能体启动", "Subagent Start") },
    { value: "subagent_stop", label: t("子智能体停止", "Subagent Stop") },
    { value: "task_created", label: t("任务创建", "Task Created") },
    { value: "task_completed", label: t("任务完成", "Task Completed") },
    { value: "worktree_create", label: t("工作树创建", "Worktree Create") },
    { value: "permission_request", label: t("请求权限", "Permission Request") },
    { value: "permission_denied", label: t("拒绝权限", "Permission Denied") },
    { value: "user_prompt_submit", label: t("提交提示词", "User Prompt Submit") },
    { value: "context_compressed", label: t("上下文已压缩", "Context Compressed") },
    { value: "stop", label: t("停止", "Stop") }
  ];

  const openAddModal = () => {
    setModalMode("add");
    setEditingIndex(null);
    setFormName("");
    setFormCommand("");
    setFormEvents([]);
    setFormTimeout(10);
    setFormCanBlock(false);
    setFormToolFilter("");
    setIsModalOpen(true);
  };

  const openEditModal = (hook: HookEntry, index: number) => {
    setModalMode("edit");
    setEditingIndex(index);
    setFormName(hook.name);
    setFormCommand(hook.command);
    setFormEvents(hook.events || []);
    setFormTimeout(hook.timeoutSecs || 10);
    setFormCanBlock(hook.canBlock || false);
    setFormToolFilter((hook.toolFilter || []).join(", "));
    setIsModalOpen(true);
  };

  const handleSave = () => {
    if (!formName.trim()) {
      setStatusText(t("钩子名称不能为空。", "Hook name cannot be empty."));
      return;
    }
    if (!formCommand.trim()) {
      setStatusText(t("执行指令不能为空。", "Command cannot be empty."));
      return;
    }
    if (formEvents.length === 0) {
      setStatusText(t("请至少选择一个触发事件。", "Please select at least one trigger event."));
      return;
    }

    const parsedTools = formToolFilter
      .split(",")
      .map((t) => t.trim())
      .filter((t) => t.length > 0);

    const newHook: HookEntry = {
      name: formName.trim(),
      command: formCommand.trim(),
      events: formEvents,
      timeoutSecs: Number(formTimeout) || 10,
      canBlock: formCanBlock,
      disabled: editingIndex !== null ? hooksList[editingIndex].disabled : false,
      toolFilter: parsedTools.length > 0 ? parsedTools : undefined
    };

    let updatedList: HookEntry[];
    if (modalMode === "add") {
      updatedList = [...hooksList, newHook];
    } else {
      updatedList = hooksList.map((h, i) => (i === editingIndex ? newHook : h));
    }

    void applyHooksSettings({ ...currentSettings(), hooks: updatedList });
    setIsModalOpen(false);
  };

  const handleDeleteHook = () => {
    if (editingIndex === null) return;
    if (confirm(t(`确定要删除钩子 "${hooksList[editingIndex].name}" 吗？`, `Are you sure you want to delete hook "${hooksList[editingIndex].name}"?`))) {
      const updated = hooksList.filter((_, i) => i !== editingIndex);
      void applyHooksSettings({ ...currentSettings(), hooks: updated });
      setStatusText(t("钩子已删除。", "Hook deleted."));
      setIsModalOpen(false);
    }
  };

  const handleToggleEvent = (eventVal: string) => {
    if (formEvents.includes(eventVal)) {
      setFormEvents(formEvents.filter((e) => e !== eventVal));
    } else {
      setFormEvents([...formEvents, eventVal]);
    }
  };

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div style={{ fontSize: "12px", color: "var(--text-soft)" }}>
        {t("配置在特定生命周期事件触发时执行的自定义脚本或指令。", "Configure custom scripts or commands executed when specific lifecycle events trigger.")}
      </div>

      <div className="theme-card" style={{ padding: "12px 16px" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
            <div
              style={{
                width: "32px",
                height: "32px",
                borderRadius: "var(--radius)",
                background: "var(--field)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: "var(--accent)"
              }}
            >
              <GitBranch size={18} />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
              <span style={{ fontWeight: "650", fontSize: "13px", color: "var(--text)" }}>{t("启用钩子系统", "Enable Hooks")}</span>
              <span style={{ fontSize: "11px", color: "var(--text-soft)" }}>
                {t("允许在事件发生时运行注册的钩子脚本", "Allow running registered hook scripts on events")}
              </span>
            </div>
          </div>
          <label className="switch-wrapper">
            <input
              type="checkbox"
              checked={hooksEnabled}
              onChange={(e) => {
                void applyHooksSettings({ ...currentSettings(), enabled: e.target.checked });
              }}
            />
            <span className="switch-slider" />
          </label>
        </div>
        {statusText && (
          <div style={{ fontSize: "11px", color: "var(--text-soft)" }}>
            {statusText}
          </div>
        )}
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("钩子列表", "Hooks")}
          </span>
          <button
            onClick={openAddModal}
            type="button"
            className="secondary-button"
            style={{
              display: "flex",
              alignItems: "center",
              gap: "6px",
              paddingInline: "12px",
              height: "28px",
              background: "var(--panel-raised)",
              borderColor: "var(--line)"
            }}
          >
            <Plus size={14} />
            <span>{t("添加钩子", "Add Hook")}</span>
          </button>
        </div>

        <div className="theme-card" style={{ padding: hooksList.length > 0 ? "8px 0" : "24px 16px" }}>
          {hooksList.length === 0 ? (
            <div style={{ textAlign: "center", color: "var(--text-soft)", fontSize: "13px" }}>
              {t("暂无配置的事件钩子", "No configured hooks")}
            </div>
          ) : (
            hooksList.map((hook, idx) => (
              <div key={idx}>
                {idx > 0 && <div className="divider" style={{ margin: "4px 16px" }} />}
                <div className="form-row" style={{ minHeight: "56px" }}>
                  <div className="row-info" style={{ gap: "4px" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                      <span className="row-label" style={{ fontSize: "13.5px", fontWeight: "600" }}>
                        {hook.name}
                      </span>
                      {hook.canBlock && (
                        <span
                          style={{
                            fontSize: "9px",
                            fontWeight: "bold",
                            padding: "1px 5px",
                            borderRadius: "4px",
                            background: "rgba(255, 85, 85, 0.15)",
                            color: "#FF5555"
                          }}
                        >
                          {t("可阻塞", "Blocking")}
                        </span>
                      )}
                    </div>

                    <div style={{ display: "flex", gap: "4px", flexWrap: "wrap", marginBlock: "2px" }}>
                      {hook.events.map((ev) => (
                        <span
                          key={ev}
                          style={{
                            fontSize: "10px",
                            background: "var(--field)",
                            border: "1px solid var(--line-soft)",
                            padding: "1px 6px",
                            borderRadius: "4px",
                            fontFamily: "var(--font-code)",
                            color: "var(--text-soft)"
                          }}
                        >
                          {ev}
                        </span>
                      ))}
                    </div>

                    <span
                      className="row-desc"
                      style={{ fontSize: "11px", color: "var(--text-soft)", fontFamily: "var(--font-code)", opacity: 0.8 }}
                    >
                      $ {hook.command} {hook.timeoutSecs ? `(${hook.timeoutSecs}s timeout)` : ""}
                    </span>
                  </div>

                  <div style={{ display: "flex", alignItems: "center", gap: "16px" }}>
                    <button
                      onClick={() => openEditModal(hook, idx)}
                      type="button"
                      style={{
                        background: "transparent",
                        border: "none",
                        cursor: "pointer",
                        color: "var(--text-soft)",
                        display: "flex",
                        alignItems: "center",
                        padding: "4px",
                        transition: "color 150ms"
                      }}
                      onMouseEnter={(e) => (e.currentTarget.style.color = "var(--text)")}
                      onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-soft)")}
                    >
                      <Settings size={15} />
                    </button>

                    <label className="switch-wrapper">
                      <input type="checkbox" checked={!hook.disabled} onChange={() => handleToggleHook(idx)} />
                      <span className="switch-slider" />
                    </label>
                  </div>
                </div>
              </div>
            ))
          )}
        </div>
      </div>

      {isModalOpen && (
        <div
          style={{
            position: "fixed",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            background: "rgba(0, 0, 0, 0.65)",
            backdropFilter: "blur(6px)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 1000,
            animation: "fadeIn 200ms ease"
          }}
        >
          <div
            className="theme-card"
            style={{
              width: "500px",
              maxHeight: "85vh",
              background: "var(--panel)",
              border: "1px solid var(--line)",
              borderRadius: "var(--radius)",
              display: "flex",
              flexDirection: "column",
              boxShadow: "0 24px 38px 3px rgba(0,0,0,0.4), 0 9px 46px 8px rgba(0,0,0,0.3)",
              overflow: "hidden"
            }}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                padding: "16px 20px",
                borderBottom: "1px solid var(--line-soft)",
                background: "var(--chrome)"
              }}
            >
              <span style={{ fontWeight: "600", fontSize: "14.5px", color: "var(--text)" }}>
                {modalMode === "add" ? t("添加事件钩子", "Add Lifecycle Hook") : t("配置事件钩子", "Configure Lifecycle Hook")}
              </span>
              <button
                onClick={() => setIsModalOpen(false)}
                type="button"
                style={{
                  background: "transparent",
                  border: "none",
                  cursor: "pointer",
                  color: "var(--text-soft)",
                  display: "flex",
                  alignItems: "center",
                  padding: "4px"
                }}
              >
                <span style={{ display: "inline-grid", placeItems: "center" }}><X size={16} /></span>
              </button>
            </div>

            <div style={{ padding: "20px", overflowY: "auto", display: "flex", flexDirection: "column", gap: "16px", flex: 1 }}>
              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
                  {t("钩子名称", "Hook Name")}
                </label>
                <input
                  type="text"
                  value={formName}
                  onChange={(e) => setFormName(e.target.value)}
                  placeholder="e.g. Run Linter"
                  style={{
                    background: "var(--field)",
                    border: "1px solid var(--line-soft)",
                    borderRadius: "var(--radius)",
                    padding: "8px 12px",
                    fontSize: "12.5px",
                    color: "var(--text)",
                    outline: "none"
                  }}
                />
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
                  {t("执行指令", "Command")}
                </label>
                <input
                  type="text"
                  value={formCommand}
                  onChange={(e) => setFormCommand(e.target.value)}
                  placeholder="e.g. npm run test"
                  style={{
                    background: "var(--field)",
                    border: "1px solid var(--line-soft)",
                    borderRadius: "var(--radius)",
                    padding: "8px 12px",
                    fontSize: "12.5px",
                    color: "var(--text)",
                    outline: "none",
                    fontFamily: "var(--font-code)"
                  }}
                />
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
                  {t("触发生命周期事件", "Trigger Events")}
                </label>
                <div
                  style={{
                    display: "grid",
                    gridTemplateColumns: "1fr 1fr",
                    gap: "8px",
                    maxHeight: "150px",
                    overflowY: "auto",
                    padding: "10px",
                    background: "var(--field)",
                    border: "1px solid var(--line-soft)",
                    borderRadius: "var(--radius)"
                  }}
                >
                  {eventOptions.map((opt) => {
                    const isChecked = formEvents.includes(opt.value);
                    return (
                      <label
                        key={opt.value}
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: "8px",
                          fontSize: "11.5px",
                          color: "var(--text)",
                          cursor: "pointer",
                          padding: "4px",
                          borderRadius: "4px",
                          background: isChecked ? "var(--accent-muted)" : "transparent"
                        }}
                      >
                        <input
                          type="checkbox"
                          checked={isChecked}
                          onChange={() => handleToggleEvent(opt.value)}
                          style={{ accentColor: "var(--accent)" }}
                        />
                        <span>
                          {opt.label}{" "}
                          <code style={{ fontSize: "9.5px", color: "var(--text-soft)" }}>({opt.value})</code>
                        </span>
                      </label>
                    );
                  })}
                </div>
              </div>

              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "16px" }}>
                <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                  <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
                    {t("超时时间 (秒)", "Timeout (seconds)")}
                  </label>
                  <input
                    type="number"
                    value={formTimeout}
                    min={1}
                    onChange={(e) => setFormTimeout(Number(e.target.value))}
                    style={{
                      background: "var(--field)",
                      border: "1px solid var(--line-soft)",
                      borderRadius: "var(--radius)",
                      padding: "8px 12px",
                      fontSize: "12.5px",
                      color: "var(--text)",
                      outline: "none"
                    }}
                  />
                </div>

                <div style={{ display: "flex", flexDirection: "column", gap: "6px", justifyContent: "center" }}>
                  <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", marginBottom: "4px" }}>
                    {t("允许阻塞执行", "Can Block Execution")}
                  </span>
                  <div style={{ display: "flex", alignItems: "center", height: "34px" }}>
                    <label className="switch-wrapper">
                      <input type="checkbox" checked={formCanBlock} onChange={(e) => setFormCanBlock(e.target.checked)} />
                      <span className="switch-slider" />
                    </label>
                  </div>
                </div>
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
                  {t("工具过滤器 (逗号分隔，可选)", "Tool Filter (Comma-separated, Optional)")}
                </label>
                <input
                  type="text"
                  value={formToolFilter}
                  onChange={(e) => setFormToolFilter(e.target.value)}
                  placeholder="e.g. view_file, replace_file_content"
                  style={{
                    background: "var(--field)",
                    border: "1px solid var(--line-soft)",
                    borderRadius: "var(--radius)",
                    padding: "8px 12px",
                    fontSize: "12.5px",
                    color: "var(--text)",
                    outline: "none",
                    fontFamily: "var(--font-code)"
                  }}
                />
              </div>
            </div>

            <div
              style={{
                display: "flex",
                justifyContent: modalMode === "edit" ? "space-between" : "flex-end",
                alignItems: "center",
                padding: "12px 20px",
                borderTop: "1px solid var(--line-soft)",
                background: "var(--chrome)"
              }}
            >
              {modalMode === "edit" && (
                <button
                  onClick={handleDeleteHook}
                  type="button"
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "6px",
                    background: "transparent",
                    border: "1px solid rgba(224, 80, 80, 0.2)",
                    borderRadius: "var(--radius)",
                    padding: "6px 12px",
                    fontSize: "12px",
                    color: "oklch(67% 0.15 28)",
                    cursor: "pointer",
                    transition: "all 150ms ease"
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background = "rgba(224, 80, 80, 0.1)";
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = "transparent";
                  }}
                >
                  <Trash2 size={13} />
                  <span>{t("删除", "Delete")}</span>
                </button>
              )}

              <div style={{ display: "flex", gap: "10px" }}>
                <button
                  onClick={() => setIsModalOpen(false)}
                  type="button"
                  style={{
                    background: "transparent",
                    border: "none",
                    padding: "6px 12px",
                    fontSize: "12px",
                    color: "var(--text-soft)",
                    cursor: "pointer"
                  }}
                >
                  {t("取消", "Cancel")}
                </button>
                <button
                  onClick={handleSave}
                  type="button"
                  style={{
                    background: "var(--accent)",
                    color: "var(--bg)",
                    border: "none",
                    borderRadius: "var(--radius)",
                    padding: "6px 16px",
                    fontSize: "12px",
                    fontWeight: "600",
                    cursor: "pointer"
                  }}
                >
                  {t("保存", "Save")}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
