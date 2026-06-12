import React, { useState } from "react";
import { Trash2 } from "lucide-react";

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
