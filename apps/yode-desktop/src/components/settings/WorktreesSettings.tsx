import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Database, FolderGit2, HardDrive, RefreshCw, ShieldCheck, Trash2 } from "lucide-react";
import { isTauriRuntime, loadDesktopSetting, saveDesktopSetting } from "../../lib/desktopSettings";

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
  const [statusText, setStatusText] = useState("");

  const [worktrees, setWorktrees] = useState<WorktreeInfo[]>([]);

  const updateVal = (key: string, val: unknown) => {
    void saveDesktopSetting(key, val);
  };

  useEffect(() => {
    void loadDesktopSetting("yode-worktrees-base-dir", baseDir).then(setBaseDir);
    void loadDesktopSetting("yode-worktrees-auto-delete-session-end", autoDeleteOnSessionEnd).then(setAutoDeleteOnSessionEnd);
    void loadDesktopSetting("yode-worktrees-preserve-uncommitted", preserveUncommitted).then(setPreserveUncommitted);
    void loadDesktopSetting("yode-worktrees-clean-unused-cache", cleanUnusedCache).then(setCleanUnusedCache);
    if (isTauriRuntime()) {
      invoke<WorktreeInfo[]>("worktrees_list")
        .then(setWorktrees)
        .catch((err) => {
          console.error(err);
          setStatusText(t("读取 git 工作树失败。", "Failed to load git worktrees."));
        });
    } else {
      setWorktrees([]);
    }
  }, []);

  const refreshWorktrees = async () => {
    if (!isTauriRuntime()) return;
    const list = await invoke<WorktreeInfo[]>("worktrees_list");
    setWorktrees(list);
  };

  const handlePruneIdle = async () => {
    if (!isTauriRuntime()) {
      setStatusText(t("请在桌面端中清理真实工作树。", "Open the desktop app to prune real worktrees."));
      return;
    }
    try {
      const result = await invoke<{ ok: boolean; message: string }>("worktrees_prune_idle");
      setStatusText(result.message);
      await refreshWorktrees();
    } catch (err) {
      console.error(err);
      setStatusText(t("清理闲置工作树失败。", "Failed to prune idle worktrees."));
    }
  };

  const handleDeleteWorktree = async (id: string, branch: string) => {
    if (
      confirm(
        t(
          `确定要删除并注销工作树 "${branch}" 吗？这会删除其本地目录中的所有临时更改。`,
          `Are you sure you want to delete and unregister worktree "${branch}"? This will delete all temporary changes in its local folder.`
        )
      )
    ) {
      if (isTauriRuntime()) {
        try {
          const target = worktrees.find((w) => w.id === id)?.path || id;
          const result = await invoke<{ ok: boolean; message: string }>("worktree_delete", { path: target });
          setStatusText(result.message);
          await refreshWorktrees();
        } catch (err) {
          console.error(err);
          setStatusText(t("删除工作树失败。", "Failed to delete worktree."));
        }
      } else {
        setStatusText(t("请在桌面端中删除真实工作树。", "Open the desktop app to delete real worktrees."));
      }
    }
  };

  const activeCount = worktrees.filter((w) => w.status === "Active").length;
  const idleCount = worktrees.filter((w) => w.status === "Idle").length;
  const totalSize = worktrees.reduce((sum, wt) => {
    const value = Number.parseFloat(wt.size);
    if (!Number.isFinite(value)) return sum;
    return sum + value;
  }, 0);
  const totalSizeLabel = totalSize > 1024
    ? `${(totalSize / 1024).toFixed(1)} GB`
    : `${Math.round(totalSize)} MB`;

  return (
    <div className="appearance-container worktrees-settings">
      <div className="worktrees-page-head">
        <div>
          <h1>{t("工作树", "Worktrees")}</h1>
          <p>
            {t("管理隔离工作目录、清理策略和缓存占用，让 Yode 可以并行处理多个任务。", "Manage isolated work directories, cleanup policies, and cache usage so Yode can work on multiple tasks in parallel.")}
          </p>
        </div>
        <div className="worktrees-stats">
          <span>{t("运行中", "Active")} <strong>{activeCount}</strong></span>
          <span>{t("闲置", "Idle")} <strong>{idleCount}</strong></span>
          <span>{t("占用", "Size")} <strong>{totalSizeLabel}</strong></span>
        </div>
      </div>

      <section className="worktrees-path-panel">
        <div className="worktrees-path-copy">
          <span className="worktrees-section-icon">
            <Database size={18} />
          </span>
          <div>
            <h2>{t("基准存储目录", "Worktree base directory")}</h2>
            <p>{t("工作树的默认拉取和生成根路径。建议放在主仓库之外，便于清理和隔离。", "Default root path for generated worktrees. Keep it outside the main repo for easier cleanup and isolation.")}</p>
          </div>
        </div>
        <div className="worktrees-path-field">
          <input
            type="text"
            value={baseDir}
            onChange={(e) => {
              setBaseDir(e.target.value);
              updateVal("yode-worktrees-base-dir", e.target.value);
            }}
            placeholder="~/.yode/worktrees"
          />
        </div>
      </section>

      <section className="worktrees-section">
        <div className="worktrees-section-head">
          <span>{t("清理策略", "Cleanup policy")}</span>
          <em>{t("安全优先，必要时保留可恢复状态", "Safety first, keeping recoverable state when needed")}</em>
        </div>
        <div className="worktrees-policy-list">
          <div className="worktrees-policy-row">
            <div className="worktrees-policy-main">
              <ShieldCheck size={18} />
              <div>
                <strong>{t("会话结束时自动清理", "Auto-delete on session end")}</strong>
                <span>{t("对话结束或归档后，自动注销并清除相关工作树。", "Unregister and remove associated worktrees after a session completes or archives.")}</span>
              </div>
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

          <div className="worktrees-policy-row">
            <div className="worktrees-policy-main">
              <FolderGit2 size={18} />
              <div>
                <strong>{t("保留未提交的修改", "Preserve uncommitted changes")}</strong>
                <span>{t("清理前自动 stash 本地修改，降低误删风险。", "Automatically stash local changes before cleanup to reduce data-loss risk.")}</span>
              </div>
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

          <div className="worktrees-policy-row">
            <div className="worktrees-policy-main">
              <HardDrive size={18} />
              <div>
                <strong>{t("清理包管理器缓存", "Clean package caches")}</strong>
                <span>{t("删除闲置工作树中的 node_modules、target 等缓存目录。", "Remove node_modules, target, and similar caches from idle worktrees.")}</span>
              </div>
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
      </section>

      <section className="worktrees-section">
        <div className="worktrees-section-head worktrees-list-head">
          <div>
            <span>{t("活动中的工作树", "Active worktrees")}</span>
            <em>{t("当前会话和分支生成的隔离目录", "Isolated directories created for current sessions and branches")}</em>
          </div>
          <button
            onClick={handlePruneIdle}
            type="button"
            className="worktrees-prune-button"
            disabled={idleCount === 0}
          >
            <RefreshCw size={14} />
            <span>{t("清理闲置工作树", "Prune Idle")}</span>
          </button>
        </div>

        <div className="worktrees-list-card">
          {worktrees.length === 0 ? (
            <div className="worktrees-empty">
              {t("暂无活动的工作树", "No active worktrees")}
            </div>
          ) : (
            worktrees.map((wt) => (
              <div key={wt.id} className="worktree-row">
                <div className="worktree-branch-cell">
                  <strong>{wt.branch}</strong>
                  <span>{wt.path}</span>
                </div>
                <div className="worktree-meta-cell">
                  <span className={`worktree-status-pill ${wt.status === "Active" ? "active" : "idle"}`}>
                    {wt.status === "Active" ? t("运行中", "Active") : t("闲置", "Idle")}
                  </span>
                  <span className="worktree-size">{wt.size}</span>
                  <button
                    onClick={() => handleDeleteWorktree(wt.id, wt.branch)}
                    type="button"
                    className="worktree-delete-button"
                    aria-label={t(`删除工作树 ${wt.branch}`, `Delete worktree ${wt.branch}`)}
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
        {statusText && (
          <div className="worktrees-status-text">
            {statusText}
          </div>
        )}
      </section>
    </div>
  );
}
