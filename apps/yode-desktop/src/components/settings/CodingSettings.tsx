import React, { useState } from "react";
import { Plus, Trash2, X, Settings, GitBranch, Folder } from "lucide-react";

// ----------------------------------------------------
// Hooks Settings Component
// ----------------------------------------------------
interface HookEntry {
  name: string;
  events: string[];
  command: string;
  timeout_secs: number;
  can_block: boolean;
  disabled: boolean;
  tool_filter?: string[];
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

  const [hooksList, setHooksList] = useState<HookEntry[]>(() => {
    const saved = localStorage.getItem("yode-hooks-list");
    if (saved) {
      try {
        return JSON.parse(saved);
      } catch (e) {
        // use defaults
      }
    }
    return [
      {
        name: "Pre-commit check",
        events: ["pre_turn"],
        command: "npm run lint",
        timeout_secs: 15,
        can_block: true,
        disabled: false
      },
      {
        name: "Auto-format code",
        events: ["task_completed"],
        command: "cargo fmt",
        timeout_secs: 10,
        can_block: false,
        disabled: false
      }
    ];
  });

  const saveHooks = (list: HookEntry[]) => {
    setHooksList(list);
    localStorage.setItem("yode-hooks-list", JSON.stringify(list));
  };

  const handleToggleHook = (index: number) => {
    const updated = [...hooksList];
    updated[index].disabled = !updated[index].disabled;
    saveHooks(updated);
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
    setFormTimeout(hook.timeout_secs || 10);
    setFormCanBlock(hook.can_block || false);
    setFormToolFilter((hook.tool_filter || []).join(", "));
    setIsModalOpen(true);
  };

  const handleSave = () => {
    if (!formName.trim()) {
      alert(t("钩子名称不能为空", "Hook name cannot be empty"));
      return;
    }
    if (!formCommand.trim()) {
      alert(t("执行指令不能为空", "Command cannot be empty"));
      return;
    }
    if (formEvents.length === 0) {
      alert(t("请至少选择一个触发事件", "Please select at least one trigger event"));
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
      timeout_secs: Number(formTimeout) || 10,
      can_block: formCanBlock,
      disabled: editingIndex !== null ? hooksList[editingIndex].disabled : false,
      tool_filter: parsedTools.length > 0 ? parsedTools : undefined
    };

    let updatedList: HookEntry[];
    if (modalMode === "add") {
      updatedList = [...hooksList, newHook];
    } else {
      updatedList = hooksList.map((h, i) => (i === editingIndex ? newHook : h));
    }

    saveHooks(updatedList);
    setIsModalOpen(false);
  };

  const handleDeleteHook = () => {
    if (editingIndex === null) return;
    if (confirm(t(`确定要删除钩子 "${hooksList[editingIndex].name}" 吗？`, `Are you sure you want to delete hook "${hooksList[editingIndex].name}"?`))) {
      const updated = hooksList.filter((_, i) => i !== editingIndex);
      saveHooks(updated);
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
                setHooksEnabled(e.target.checked);
                localStorage.setItem("yode-hooks-enabled", String(e.target.checked));
              }}
            />
            <span className="switch-slider" />
          </label>
        </div>
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
                      {hook.can_block && (
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
                      $ {hook.command} {hook.timeout_secs ? `(${hook.timeout_secs}s timeout)` : ""}
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
                <X size={16} />
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

// ----------------------------------------------------
// Git Settings Component
// ----------------------------------------------------
export function GitSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [branchPrefix, setBranchPrefix] = useState(() => {
    return localStorage.getItem("yode-git-branch-prefix") || "yode/";
  });
  const [mergeMethod, setMergeMethod] = useState(() => {
    return localStorage.getItem("yode-git-merge-method") || "merge";
  });
  const [showPrIcons, setShowPrIcons] = useState(() => {
    return localStorage.getItem("yode-git-show-pr-icons") !== "false";
  });
  const [alwaysForcePush, setAlwaysForcePush] = useState(() => {
    return localStorage.getItem("yode-git-always-force-push") === "true";
  });
  const [createDraftPrs, setCreateDraftPrs] = useState(() => {
    return localStorage.getItem("yode-git-create-draft-prs") !== "false";
  });
  const [autoDeleteWorktrees, setAutoDeleteWorktrees] = useState(() => {
    return localStorage.getItem("yode-git-auto-delete-worktrees") !== "false";
  });
  const [autoDeleteLimit, setAutoDeleteLimit] = useState(() => {
    return Number(localStorage.getItem("yode-git-auto-delete-limit") || "15");
  });
  const [commitInstructions, setCommitInstructions] = useState(() => {
    return localStorage.getItem("yode-git-commit-instructions") || "";
  });
  const [prInstructions, setPrInstructions] = useState(() => {
    return localStorage.getItem("yode-git-pr-instructions") || "";
  });

  const updateVal = (key: string, val: any) => {
    localStorage.setItem(key, String(val));
  };

  const handleSaveCommitInstructions = () => {
    updateVal("yode-git-commit-instructions", commitInstructions);
    alert(t("提交说明配置已成功保存！", "Commit instructions saved successfully!"));
  };

  const handleSavePrInstructions = () => {
    updateVal("yode-git-pr-instructions", prInstructions);
    alert(t("PR 说明配置已成功保存！", "PR instructions saved successfully!"));
  };

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div className="theme-card">
        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("分支前缀", "Branch prefix")}</span>
            <span className="row-desc">{t("在 Yode 中创建新分支时使用的前缀", "Prefix used when creating new branches in Yode")}</span>
          </div>
          <input
            type="text"
            value={branchPrefix}
            onChange={(e) => {
              setBranchPrefix(e.target.value);
              updateVal("yode-git-branch-prefix", e.target.value);
            }}
            placeholder="yode/"
            style={{
              background: "var(--field)",
              border: "1px solid var(--line-soft)",
              borderRadius: "var(--radius)",
              padding: "6px 12px",
              fontSize: "12.5px",
              color: "var(--text)",
              outline: "none",
              fontFamily: "var(--font-code)",
              width: "200px"
            }}
          />
        </div>

        <div className="divider" />

        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("合并拉取请求方式", "Pull request merge method")}</span>
            <span className="row-desc">{t("选择 Yode 合并拉取请求的方式", "Choose how Yode merges pull requests")}</span>
          </div>
          <div className="segmented-control">
            <button
              className={`segmented-btn ${mergeMethod === "merge" ? "active" : ""}`}
              onClick={() => {
                setMergeMethod("merge");
                updateVal("yode-git-merge-method", "merge");
              }}
              type="button"
            >
              {t("合并", "Merge")}
            </button>
            <button
              className={`segmented-btn ${mergeMethod === "squash" ? "active" : ""}`}
              onClick={() => {
                setMergeMethod("squash");
                updateVal("yode-git-merge-method", "squash");
              }}
              type="button"
            >
              {t("扁平化合并", "Squash")}
            </button>
          </div>
        </div>

        <div className="divider" />

        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("在侧边栏显示 PR 图标", "Show PR icons in sidebar")}</span>
            <span className="row-desc">{t("在侧边栏的对话行中显示 PR 状态图标", "Display PR status icons on chat rows in the sidebar")}</span>
          </div>
          <label className="switch-wrapper">
            <input
              type="checkbox"
              checked={showPrIcons}
              onChange={(e) => {
                setShowPrIcons(e.target.checked);
                updateVal("yode-git-show-pr-icons", e.target.checked);
              }}
            />
            <span className="switch-slider" />
          </label>
        </div>

        <div className="divider" />

        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("始终强制推送", "Always force push")}</span>
            <span className="row-desc">{t("从 Yode 推送时使用 --force-with-lease 选项", "Use --force-with-lease when pushing from Yode")}</span>
          </div>
          <label className="switch-wrapper">
            <input
              type="checkbox"
              checked={alwaysForcePush}
              onChange={(e) => {
                setAlwaysForcePush(e.target.checked);
                updateVal("yode-git-always-force-push", e.target.checked);
              }}
            />
            <span className="switch-slider" />
          </label>
        </div>

        <div className="divider" />

        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("创建草稿拉取请求", "Create draft pull requests")}</span>
            <span className="row-desc">{t("从 Yode 创建 PR 时默认使用草稿拉取请求", "Use draft pull requests by default when creating PRs from Yode")}</span>
          </div>
          <label className="switch-wrapper">
            <input
              type="checkbox"
              checked={createDraftPrs}
              onChange={(e) => {
                setCreateDraftPrs(e.target.checked);
                updateVal("yode-git-create-draft-prs", e.target.checked);
              }}
            />
            <span className="switch-slider" />
          </label>
        </div>

        <div className="divider" />

        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("自动删除旧工作树", "Automatically delete old worktrees")}</span>
            <span className="row-desc">
              {t(
                "推荐大多数用户开启。如果您想自己管理旧工作树和磁盘空间，请关闭此项",
                "Recommended for most users. Turn this off only if you want to manage old worktrees and disk usage yourself"
              )}
            </span>
          </div>
          <label className="switch-wrapper">
            <input
              type="checkbox"
              checked={autoDeleteWorktrees}
              onChange={(e) => {
                setAutoDeleteWorktrees(e.target.checked);
                updateVal("yode-git-auto-delete-worktrees", e.target.checked);
              }}
            />
            <span className="switch-slider" />
          </label>
        </div>

        <div className="divider" />

        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("自动删除上限", "Auto-delete limit")}</span>
            <span className="row-desc">
              {t(
                "在自动清理前保留的 Yode 工作树数量。Yode 在删除前会进行快照，因此已清理的工作树始终可以恢复",
                "Number of Yode worktrees to keep before older ones are pruned automatically. Yode snapshots worktrees before deleting, so pruned worktrees should always be restorable"
              )}
            </span>
          </div>
          <input
            type="number"
            value={autoDeleteLimit}
            onChange={(e) => {
              const val = Number(e.target.value) || 1;
              setAutoDeleteLimit(val);
              updateVal("yode-git-auto-delete-limit", val);
            }}
            min={1}
            style={{
              background: "var(--field)",
              border: "1px solid var(--line-soft)",
              borderRadius: "var(--radius)",
              padding: "6px 12px",
              fontSize: "12.5px",
              color: "var(--text)",
              outline: "none",
              width: "100px"
            }}
          />
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("提交说明", "Commit instructions")}
          </span>
          <button
            onClick={handleSaveCommitInstructions}
            type="button"
            className="secondary-button"
            style={{
              paddingInline: "12px",
              height: "24px",
              fontSize: "11px",
              fontWeight: "600"
            }}
          >
            {t("保存", "Save")}
          </button>
        </div>
        <span style={{ fontSize: "11px", color: "var(--text-soft)", marginTop: "-4px" }}>
          {t("添加到提交信息生成提示词中", "Added to commit message generation prompts")}
        </span>
        <textarea
          value={commitInstructions}
          onChange={(e) => setCommitInstructions(e.target.value)}
          placeholder={t("添加提交信息指南...", "Add commit message guidance...")}
          style={{
            width: "100%",
            height: "100px",
            background: "var(--field)",
            border: "1px solid var(--line-soft)",
            borderRadius: "var(--radius)",
            padding: "10px",
            fontSize: "12px",
            color: "var(--text)",
            outline: "none",
            resize: "vertical",
            fontFamily: "inherit"
          }}
        />
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("拉取请求说明", "Pull request instructions")}
          </span>
          <button
            onClick={handleSavePrInstructions}
            type="button"
            className="secondary-button"
            style={{
              paddingInline: "12px",
              height: "24px",
              fontSize: "11px",
              fontWeight: "600"
            }}
          >
            {t("保存", "Save")}
          </button>
        </div>
        <span style={{ fontSize: "11px", color: "var(--text-soft)", marginTop: "-4px" }}>
          {t("添加到 PR 标题/描述生成提示词中", "Added to PR title/description generation prompts")}
        </span>
        <textarea
          value={prInstructions}
          onChange={(e) => setPrInstructions(e.target.value)}
          placeholder={t("添加 PR 标题/描述指南...", "Add PR title/description guidance...")}
          style={{
            width: "100%",
            height: "100px",
            background: "var(--field)",
            border: "1px solid var(--line-soft)",
            borderRadius: "var(--radius)",
            padding: "10px",
            fontSize: "12px",
            color: "var(--text)",
            outline: "none",
            resize: "vertical",
            fontFamily: "inherit"
          }}
        />
      </div>
    </div>
  );
}

// ----------------------------------------------------
// Environments Settings Component
// ----------------------------------------------------
interface ProjectEnvironment {
  name: string;
  subtext?: string;
  path?: string;
  setupCommand?: string;
  execMode?: "host" | "docker" | "virtualenv";
  envVars?: Array<{ key: string; value: string }>;
}

export function EnvironmentsSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [projects, setProjects] = useState<ProjectEnvironment[]>(() => {
    const saved = localStorage.getItem("yode-environments-projects");
    if (saved) {
      try {
        return JSON.parse(saved);
      } catch (e) {
        // use default list
      }
    }
    return [
      { name: "langchain学习", setupCommand: "pip install -r requirements.txt", execMode: "virtualenv" },
      { name: "yode", subtext: "anYuJia", setupCommand: "cargo build", execMode: "host" },
      { name: "简历", setupCommand: "npm install && npm run build", execMode: "host" },
      { name: "11", subtext: "anpeny", execMode: "host" },
      { name: "lh", subtext: "anYuJia", execMode: "host" },
      { name: "douyin", execMode: "host" },
      { name: "datasearch", execMode: "host" },
      { name: "clear", execMode: "host" },
      { name: "syncFile", subtext: "anYuJia", execMode: "host" },
      { name: "26年5月", execMode: "host" },
      { name: "Easydict", subtext: "anYuJia", execMode: "host" }
    ];
  });

  const [expandedIndex, setExpandedIndex] = useState<number | null>(null);

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [formName, setFormName] = useState("");
  const [formSubtext, setFormSubtext] = useState("");
  const [formPath, setFormPath] = useState("");

  const saveProjects = (list: ProjectEnvironment[]) => {
    setProjects(list);
    localStorage.setItem("yode-environments-projects", JSON.stringify(list));
  };

  const handleAddProject = () => {
    if (!formName.trim()) {
      alert(t("项目名称不能为空", "Project name cannot be empty"));
      return;
    }
    const newProj: ProjectEnvironment = {
      name: formName.trim(),
      subtext: formSubtext.trim() || undefined,
      path: formPath.trim() || undefined,
      execMode: "host",
      setupCommand: "",
      envVars: []
    };
    saveProjects([...projects, newProj]);
    setIsModalOpen(false);
    setFormName("");
    setFormSubtext("");
    setFormPath("");
  };

  const handleDeleteProject = (index: number) => {
    if (confirm(t(`确定要删除项目 "${projects[index].name}" 吗？`, `Are you sure you want to delete project "${projects[index].name}"?`))) {
      const updated = projects.filter((_, i) => i !== index);
      saveProjects(updated);
      if (expandedIndex === index) {
        setExpandedIndex(null);
      } else if (expandedIndex !== null && expandedIndex > index) {
        setExpandedIndex(expandedIndex - 1);
      }
    }
  };

  const handleSaveEnvConfig = (index: number, updatedProj: ProjectEnvironment) => {
    const updated = projects.map((p, i) => (i === index ? updatedProj : p));
    saveProjects(updated);
    alert(t("项目环境配置保存成功！", "Project environment configuration saved successfully!"));
  };

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div style={{ fontSize: "12px", color: "var(--text-soft)", display: "flex", flexDirection: "column", gap: "4px" }}>
        <span>
          {t("本地环境告诉 Yode 如何为项目配置和拉起工作树。", "Local environments tell Yode how to set up worktrees for a project.")}{" "}
          <a href="#learn-environments" style={{ color: "var(--accent)", textDecoration: "none" }}>
            {t("了解更多", "Learn more.")}
          </a>
        </span>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("选择项目", "Select a project")}
          </span>
          <button
            onClick={() => setIsModalOpen(true)}
            type="button"
            className="secondary-button"
            style={{
              paddingInline: "12px",
              height: "28px",
              background: "var(--panel-raised)",
              borderColor: "var(--line)"
            }}
          >
            <span>{t("添加项目", "Add project")}</span>
          </button>
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
          {projects.map((proj, idx) => {
            const isExpanded = expandedIndex === idx;
            return (
              <div
                key={idx}
                className="theme-card"
                style={{
                  padding: "0",
                  overflow: "hidden",
                  border: isExpanded ? "1px solid var(--accent)" : "1px solid var(--line-soft)",
                  transition: "border-color 150ms ease"
                }}
              >
                <div
                  onClick={() => setExpandedIndex(isExpanded ? null : idx)}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    padding: "12px 16px",
                    cursor: "pointer",
                    background: isExpanded ? "var(--chrome)" : "transparent",
                    transition: "background 150ms ease"
                  }}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
                    <Folder size={16} style={{ color: "var(--accent)", flexShrink: 0 }} />
                    <div style={{ display: "flex", alignItems: "baseline", gap: "8px" }}>
                      <span style={{ fontWeight: "600", fontSize: "13px", color: "var(--text)" }}>{proj.name}</span>
                      {proj.subtext && (
                        <span style={{ fontSize: "11.5px", color: "var(--text-soft)", opacity: 0.7 }}>{proj.subtext}</span>
                      )}
                    </div>
                  </div>

                  <button
                    type="button"
                    style={{
                      background: "var(--field)",
                      border: "1px solid var(--line-soft)",
                      borderRadius: "6px",
                      width: "24px",
                      height: "24px",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      color: "var(--text)",
                      cursor: "pointer",
                      padding: 0
                    }}
                  >
                    {isExpanded ? <X size={13} /> : <Plus size={13} />}
                  </button>
                </div>

                {isExpanded && (
                  <ExpandedProjectConfig
                    project={proj}
                    t={t}
                    isZh={isZh}
                    onSave={(updated) => handleSaveEnvConfig(idx, updated)}
                    onDelete={() => handleDeleteProject(idx)}
                  />
                )}
              </div>
            );
          })}
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
            zIndex: 1000
          }}
        >
          <div
            className="theme-card"
            style={{
              width: "400px",
              background: "var(--panel)",
              border: "1px solid var(--line)",
              borderRadius: "var(--radius)",
              display: "flex",
              flexDirection: "column",
              boxShadow: "0 20px 25px -5px rgb(0 0 0 / 0.3)"
            }}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                padding: "12px 16px",
                borderBottom: "1px solid var(--line-soft)",
                background: "var(--chrome)"
              }}
            >
              <span style={{ fontWeight: "600", fontSize: "13.5px", color: "var(--text)" }}>{t("添加本地项目", "Add Project")}</span>
              <button
                onClick={() => setIsModalOpen(false)}
                type="button"
                style={{ background: "transparent", border: "none", cursor: "pointer", color: "var(--text-soft)", display: "flex" }}
              >
                <X size={14} />
              </button>
            </div>

            <div style={{ padding: "16px", display: "flex", flexDirection: "column", gap: "12px" }}>
              <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
                <label style={{ fontSize: "11px", color: "var(--text-soft)", fontWeight: "600" }}>{t("项目名称", "Project Name")}</label>
                <input
                  type="text"
                  value={formName}
                  onChange={(e) => setFormName(e.target.value)}
                  placeholder="e.g. langchain-study"
                  style={{
                    background: "var(--field)",
                    border: "1px solid var(--line-soft)",
                    borderRadius: "var(--radius)",
                    padding: "8px 12px",
                    fontSize: "12px",
                    color: "var(--text)",
                    outline: "none"
                  }}
                />
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
                <label style={{ fontSize: "11px", color: "var(--text-soft)", fontWeight: "600" }}>
                  {t("子文本/拥有者 (可选)", "Owner/Subtext (Optional)")}
                </label>
                <input
                  type="text"
                  value={formSubtext}
                  onChange={(e) => setFormSubtext(e.target.value)}
                  placeholder="e.g. orgName"
                  style={{
                    background: "var(--field)",
                    border: "1px solid var(--line-soft)",
                    borderRadius: "var(--radius)",
                    padding: "8px 12px",
                    fontSize: "12px",
                    color: "var(--text)",
                    outline: "none"
                  }}
                />
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
                <label style={{ fontSize: "11px", color: "var(--text-soft)", fontWeight: "600" }}>
                  {t("本地绝对路径 (可选)", "Absolute Path (Optional)")}
                </label>
                <input
                  type="text"
                  value={formPath}
                  onChange={(e) => setFormPath(e.target.value)}
                  placeholder="e.g. /Users/username/code/project"
                  style={{
                    background: "var(--field)",
                    border: "1px solid var(--line-soft)",
                    borderRadius: "var(--radius)",
                    padding: "8px 12px",
                    fontSize: "12px",
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
                justifyContent: "flex-end",
                gap: "8px",
                padding: "10px 16px",
                borderTop: "1px solid var(--line-soft)",
                background: "var(--chrome)"
              }}
            >
              <button
                onClick={() => setIsModalOpen(false)}
                type="button"
                style={{ background: "transparent", border: "none", fontSize: "12px", color: "var(--text-soft)", cursor: "pointer" }}
              >
                {t("取消", "Cancel")}
              </button>
              <button
                onClick={handleAddProject}
                type="button"
                style={{
                  background: "var(--accent)",
                  color: "var(--bg)",
                  border: "none",
                  borderRadius: "var(--radius)",
                  padding: "4px 12px",
                  fontSize: "12px",
                  fontWeight: "600",
                  cursor: "pointer"
                }}
              >
                {t("添加", "Add")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function ExpandedProjectConfig({
  project,
  t,
  isZh,
  onSave,
  onDelete
}: {
  project: ProjectEnvironment;
  t: (zh: string, en: string) => string;
  isZh: boolean;
  onSave: (proj: ProjectEnvironment) => void;
  onDelete: () => void;
}) {
  const [setupCmd, setSetupCmd] = useState(project.setupCommand || "");
  const [execMode, setExecMode] = useState<"host" | "docker" | "virtualenv">(project.execMode || "host");
  const [envVars, setEnvVars] = useState<Array<{ key: string; value: string }>>(() => {
    return project.envVars || [];
  });

  const handleAddEnv = () => {
    setEnvVars([...envVars, { key: "", value: "" }]);
  };

  const handleRemoveEnv = (idx: number) => {
    setEnvVars(envVars.filter((_, i) => i !== idx));
  };

  const handleEnvChange = (idx: number, field: "key" | "value", val: string) => {
    const updated = [...envVars];
    updated[idx][field] = val;
    setEnvVars(updated);
  };

  const handleSave = () => {
    const validEnv = envVars.filter((pair) => pair.key.trim().length > 0);
    onSave({
      ...project,
      setupCommand: setupCmd,
      execMode: execMode,
      envVars: validEnv
    });
  };

  return (
    <div
      style={{
        padding: "16px",
        background: "var(--panel-raised)",
        display: "flex",
        flexDirection: "column",
        gap: "16px",
        borderTop: "1px solid var(--line-soft)"
      }}
    >
      <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
        <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
          {t("环境构建/准备指令", "Environment Setup Command")}
        </label>
        <input
          type="text"
          value={setupCmd}
          onChange={(e) => setSetupCmd(e.target.value)}
          placeholder="e.g. npm install, cargo build, pip install -r requirements.txt"
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
          {t("脚本运行沙箱模式", "Sandbox / Execution Mode")}
        </label>
        <div className="segmented-control" style={{ maxWidth: "360px" }}>
          <button className={`segmented-btn ${execMode === "host" ? "active" : ""}`} onClick={() => setExecMode("host")} type="button">
            {t("主机 Shell", "Host")}
          </button>
          <button className={`segmented-btn ${execMode === "docker" ? "active" : ""}`} onClick={() => setExecMode("docker")} type="button">
            Docker
          </button>
          <button className={`segmented-btn ${execMode === "virtualenv" ? "active" : ""}`} onClick={() => setExecMode("virtualenv")} type="button">
            Virtualenv
          </button>
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
            {t("项目环境变量", "Project Environment Variables")}
          </label>
          <button
            onClick={handleAddEnv}
            type="button"
            className="secondary-button"
            style={{
              fontSize: "10px",
              paddingInline: "8px",
              height: "20px",
              background: "var(--field)",
              borderColor: "var(--line-soft)"
            }}
          >
            {t("+ 添加", "+ Add")}
          </button>
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
          {envVars.map((pair, idx) => (
            <div key={idx} style={{ display: "flex", gap: "8px", alignItems: "center" }}>
              <input
                type="text"
                placeholder="KEY"
                value={pair.key}
                onChange={(e) => handleEnvChange(idx, "key", e.target.value)}
                style={{
                  flex: 1,
                  background: "var(--field)",
                  border: "1px solid var(--line-soft)",
                  borderRadius: "var(--radius)",
                  padding: "6px 10px",
                  fontSize: "11.5px",
                  color: "var(--text)",
                  outline: "none",
                  fontFamily: "var(--font-code)"
                }}
              />
              <input
                type="text"
                placeholder="Value"
                value={pair.value}
                onChange={(e) => handleEnvChange(idx, "value", e.target.value)}
                style={{
                  flex: 1.5,
                  background: "var(--field)",
                  border: "1px solid var(--line-soft)",
                  padding: "6px 10px",
                  borderRadius: "var(--radius)",
                  fontSize: "11.5px",
                  color: "var(--text)",
                  outline: "none",
                  fontFamily: "var(--font-code)"
                }}
              />
              <button
                onClick={() => handleRemoveEnv(idx)}
                type="button"
                style={{
                  background: "transparent",
                  border: "none",
                  cursor: "pointer",
                  color: "var(--text-soft)",
                  padding: "4px"
                }}
                onMouseEnter={(e) => (e.currentTarget.style.color = "oklch(67% 0.15 28)")}
                onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-soft)")}
              >
                <Trash2 size={14} />
              </button>
            </div>
          ))}
          {envVars.length === 0 && (
            <span style={{ fontSize: "11px", color: "var(--text-soft)", opacity: 0.6, fontStyle: "italic" }}>
              {t("未配置环境变量", "No environment variables")}
            </span>
          )}
        </div>
      </div>

      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginTop: "8px",
          paddingTop: "12px",
          borderTop: "1px solid var(--line-soft)"
        }}
      >
        <button
          onClick={onDelete}
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
          <span>{t("删除项目", "Delete Project")}</span>
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
          {t("保存配置", "Save Config")}
        </button>
      </div>
    </div>
  );
}

// ----------------------------------------------------
// Worktrees Settings Component
// ----------------------------------------------------
interface WorktreeInfo {
  id: string;
  branch: string;
  path: string;
  status: "Active" | "Idle";
  size: string;
}

export function WorktreesSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [baseDir, setBaseDir] = useState(() => {
    return localStorage.getItem("yode-worktrees-base-dir") || "~/.yode/worktrees";
  });
  const [autoDeleteOnSessionEnd, setAutoDeleteOnSessionEnd] = useState(() => {
    return localStorage.getItem("yode-worktrees-auto-delete-session-end") !== "false";
  });
  const [preserveUncommitted, setPreserveUncommitted] = useState(() => {
    return localStorage.getItem("yode-worktrees-preserve-uncommitted") !== "false";
  });
  const [cleanUnusedCache, setCleanUnusedCache] = useState(() => {
    return localStorage.getItem("yode-worktrees-clean-unused-cache") === "true";
  });

  const [worktrees, setWorktrees] = useState<WorktreeInfo[]>(() => {
    const saved = localStorage.getItem("yode-worktrees-list");
    if (saved) {
      try {
        return JSON.parse(saved);
      } catch (e) {
        // use defaults
      }
    }
    return [
      { id: "1", branch: "feature/auth", path: "/Users/pyu/code/yode/.worktrees/feature-auth", status: "Active", size: "142 MB" },
      { id: "2", branch: "fix/sidebar-flash", path: "/Users/pyu/code/yode/.worktrees/fix-sidebar-flash", status: "Idle", size: "98 MB" },
      { id: "3", branch: "refactor/db-migration", path: "/Users/pyu/code/yode/.worktrees/refactor-db-migration", status: "Idle", size: "210 MB" }
    ];
  });

  const saveWorktrees = (list: WorktreeInfo[]) => {
    setWorktrees(list);
    localStorage.setItem("yode-worktrees-list", JSON.stringify(list));
  };

  const updateVal = (key: string, val: any) => {
    localStorage.setItem(key, String(val));
  };

  const handlePruneIdle = () => {
    const activeOnes = worktrees.filter((w) => w.status === "Active");
    const prunedCount = worktrees.length - activeOnes.length;
    if (prunedCount === 0) {
      alert(t("暂无闲置的工作树可以清理。", "No idle worktrees to prune."));
      return;
    }
    if (confirm(t(`确定要清理全部 ${prunedCount} 个闲置工作树吗？`, `Are you sure you want to prune all ${prunedCount} idle worktrees?`))) {
      saveWorktrees(activeOnes);
      alert(t("闲置工作树清理成功！", "Idle worktrees pruned successfully!"));
    }
  };

  const handleDeleteWorktree = (id: string, branch: string) => {
    if (
      confirm(
        t(
          `确定要删除并注销工作树 "${branch}" 吗？这会删除其本地目录中的所有临时更改。`,
          `Are you sure you want to delete and unregister worktree "${branch}"? This will delete all temporary changes in its local folder.`
        )
      )
    ) {
      const updated = worktrees.filter((w) => w.id !== id);
      saveWorktrees(updated);
    }
  };

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div style={{ fontSize: "12px", color: "var(--text-soft)", display: "flex", flexDirection: "column", gap: "4px" }}>
        <span>
          {t("管理活动的工作树及缓存目录。工作树允许 Yode 在隔离的环境中并行处理多个分支或任务。", "Manage active worktrees and cached directories. Worktrees allow Yode to work on multiple branches or tasks in parallel in isolated environments.")}
        </span>
      </div>

      <div className="theme-card">
        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("基准存储目录", "Worktree base directory")}</span>
            <span className="row-desc">{t("工作树的默认拉取和生成存储根路径", "Root path where worktrees are generated and stored")}</span>
          </div>
          <input
            type="text"
            value={baseDir}
            onChange={(e) => {
              setBaseDir(e.target.value);
              updateVal("yode-worktrees-base-dir", e.target.value);
            }}
            placeholder="~/.yode/worktrees"
            style={{
              background: "var(--field)",
              border: "1px solid var(--line-soft)",
              borderRadius: "var(--radius)",
              padding: "6px 12px",
              fontSize: "12.5px",
              color: "var(--text)",
              outline: "none",
              fontFamily: "var(--font-code)",
              width: "240px"
            }}
          />
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
          {t("清理策略", "Auto-cleanup Strategy")}
        </span>
        <div className="theme-card">
          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("会话结束时自动清理", "Auto-delete on session end")}</span>
              <span className="row-desc">{t("当对话结束或归档时，自动安全注销并清除相关的工作树", "Automatically delete associated worktrees when session completes or archives")}</span>
            </div>
            <label className="switch-wrapper">
              <input
                type="checkbox"
                checked={autoDeleteOnSessionEnd}
                onChange={(e) => {
                  setAutoDeleteOnSessionEnd(e.target.checked);
                  updateVal("yode-worktrees-auto-delete-session-end", e.target.checked);
                }}
              />
              <span className="switch-slider" />
            </label>
          </div>

          <div className="divider" />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("保留未提交的修改", "Preserve uncommitted changes")}</span>
              <span className="row-desc">{t("清理前通过自动暂存（stash）保留本地修改，防止代码丢失", "Prevent code loss by automatically stashing local uncommitted changes before deleting")}</span>
            </div>
            <label className="switch-wrapper">
              <input
                type="checkbox"
                checked={preserveUncommitted}
                onChange={(e) => {
                  setPreserveUncommitted(e.target.checked);
                  updateVal("yode-worktrees-preserve-uncommitted", e.target.checked);
                }}
              />
              <span className="switch-slider" />
            </label>
          </div>

          <div className="divider" />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("清除未使用的包管理器缓存", "Clean unused package caches")}</span>
              <span className="row-desc">{t("自动清理工作树产生的 node_modules 或 target 缓存以释放磁盘空间", "Automatically prune package caches (node_modules, target) to reclaim disk space")}</span>
            </div>
            <label className="switch-wrapper">
              <input
                type="checkbox"
                checked={cleanUnusedCache}
                onChange={(e) => {
                  setCleanUnusedCache(e.target.checked);
                  updateVal("yode-worktrees-clean-unused-cache", e.target.checked);
                }}
              />
              <span className="switch-slider" />
            </label>
          </div>
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("活动中的工作树", "Active Worktrees")}
          </span>
          <button
            onClick={handlePruneIdle}
            type="button"
            className="secondary-button"
            style={{
              paddingInline: "12px",
              height: "28px",
              fontSize: "11px",
              background: "var(--panel-raised)",
              borderColor: "var(--line)"
            }}
          >
            <span>{t("清理闲置工作树", "Prune Idle")}</span>
          </button>
        </div>

        <div className="theme-card" style={{ padding: worktrees.length > 0 ? "8px 0" : "24px 16px" }}>
          {worktrees.length === 0 ? (
            <div style={{ textAlign: "center", color: "var(--text-soft)", fontSize: "13px" }}>
              {t("暂无活动的工作树", "No active worktrees")}
            </div>
          ) : (
            worktrees.map((wt, idx) => (
              <div key={wt.id}>
                {idx > 0 && <div className="divider" style={{ margin: "4px 16px" }} />}
                <div className="form-row" style={{ minHeight: "52px" }}>
                  <div className="row-info" style={{ gap: "4px" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                      <span className="row-label" style={{ fontFamily: "var(--font-code)", fontSize: "13px" }}>
                        {wt.branch}
                      </span>

                      <span
                        style={{
                          fontSize: "9px",
                          fontWeight: "bold",
                          padding: "1px 5px",
                          borderRadius: "4px",
                          background: wt.status === "Active" ? "rgba(80, 250, 123, 0.15)" : "rgba(255, 184, 108, 0.15)",
                          color: wt.status === "Active" ? "var(--success, #50FA7B)" : "#FFB86C"
                        }}
                      >
                        {wt.status === "Active" ? t("运行中", "Active") : t("闲置", "Idle")}
                      </span>

                      <span style={{ fontSize: "10.5px", color: "var(--text-soft)" }}>({wt.size})</span>
                    </div>

                    <span
                      className="row-desc"
                      style={{ fontSize: "11px", color: "var(--text-soft)", fontFamily: "var(--font-code)", opacity: 0.8 }}
                    >
                      {wt.path}
                    </span>
                  </div>

                  <button
                    onClick={() => handleDeleteWorktree(wt.id, wt.branch)}
                    type="button"
                    style={{
                      background: "transparent",
                      border: "none",
                      cursor: "pointer",
                      color: "var(--text-soft)",
                      padding: "4px"
                    }}
                    onMouseEnter={(e) => (e.currentTarget.style.color = "oklch(67% 0.15 28)")}
                    onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-soft)")}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
