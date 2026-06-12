import React, { useState } from "react";

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
                "Pruned worktrees should always be restorable because Yode snapshots them before deleting"
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
