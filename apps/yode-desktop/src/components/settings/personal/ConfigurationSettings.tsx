import React, { useState } from "react";
import { Search, Download } from "lucide-react";
import { CustomSelect } from "../../CustomSelect";

export function ConfigurationSettings({ isZh, t }: { isZh: boolean; t: (zh: string, en: string) => string }) {
  const [configScope, setConfigScope] = useState(() => localStorage.getItem("yode-config-scope") || "User config");
  const [approvalPolicy, setApprovalPolicy] = useState(() => localStorage.getItem("yode-config-approval") || "On request");
  const [sandboxSettings, setSandboxSettings] = useState(() => localStorage.getItem("yode-config-sandbox") || "Read only");
  const [exposeDeps, setExposeDeps] = useState(() => localStorage.getItem("yode-expose-deps") !== "false");

  const saveVal = (key: string, val: any) => localStorage.setItem(key, String(val));

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div style={{ fontSize: "12px", color: "var(--text-soft)" }}>
        {t("配置审批策略和沙箱设置", "Configure approval policy and sandbox settings")}{" "}
        <a href="#learn" style={{ color: "var(--accent)", textDecoration: "none" }}>
          {t("了解更多", "Learn more")}
        </a>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
        <span
          style={{
            fontSize: "11px",
            fontWeight: "700",
            color: "var(--text-soft)",
            textTransform: "uppercase",
            letterSpacing: "0.5px"
          }}
        >
          {t("自定义 config.toml 设置", "Custom config.toml settings")}
        </span>

        <div className="theme-card" style={{ padding: "16px", display: "flex", flexDirection: "column", gap: "14px" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <CustomSelect
              value={configScope}
              onChange={(val) => {
                setConfigScope(val);
                saveVal("yode-config-scope", val);
              }}
              options={[
                { value: "User config", label: t("用户配置", "User config"), avatarText: "👤" },
                { value: "Project config", label: t("项目配置", "Project config"), avatarText: "📁" }
              ]}
              style={{ minWidth: "150px" }}
            />
            <a
              href="#open"
              style={{ fontSize: "11px", color: "var(--text-soft)", textDecoration: "none" }}
              className="hover-link"
            >
              {t("打开 config.toml ↗", "Open config.toml ↗")}
            </a>
          </div>

          <div style={{ height: "1px", background: "var(--line-soft)" }} />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("审批策略", "Approval policy")}</span>
              <span className="row-desc">{t("选择 Yode 何时需要确认请求", "Choose when Yode asks for approval")}</span>
            </div>
            <CustomSelect
              value={approvalPolicy}
              onChange={(val) => {
                setApprovalPolicy(val);
                saveVal("yode-config-approval", val);
              }}
              options={[
                { value: "On request", label: t("询问确认", "On request") },
                { value: "Always auto-approve", label: t("始终自动允许", "Always auto-approve") },
                { value: "Never approve", label: t("从不允许", "Never approve") }
              ]}
              style={{ minWidth: "160px" }}
            />
          </div>

          <div style={{ height: "1px", background: "var(--line-soft)" }} />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("沙箱设置", "Sandbox settings")}</span>
              <span className="row-desc">
                {t("选择 Yode 执行命令时的文件访问权限", "Choose how much Yode can do when running commands")}
              </span>
            </div>
            <CustomSelect
              value={sandboxSettings}
              onChange={(val) => {
                setSandboxSettings(val);
                saveVal("yode-config-sandbox", val);
              }}
              options={[
                { value: "Read only", label: t("只读", "Read only") },
                { value: "Full write access", label: t("读写访问", "Full write access") },
                { value: "Restricted", label: t("限制范围", "Restricted") }
              ]}
              style={{ minWidth: "160px" }}
            />
          </div>
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
        <span
          style={{
            fontSize: "11px",
            fontWeight: "700",
            color: "var(--text-soft)",
            textTransform: "uppercase",
            letterSpacing: "0.5px"
          }}
        >
          {t("工作区依赖项", "Workspace Dependencies")}
        </span>
        <div className="theme-card">
          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("当前版本", "Current version")}</span>
            </div>
            <span style={{ fontSize: "12px", fontFamily: "var(--font-code)", color: "var(--text-muted)", alignSelf: "center" }}>
              26.601.10930
            </span>
          </div>

          <div className="divider" />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("Yode 依赖项", "Yode dependencies")}</span>
              <span className="row-desc">
                {t("允许 Yode 安装并向工作区暴露 Node.js & Python 工具", "Allow Yode to install and expose bundled Node.js and Python tools")}
              </span>
            </div>
            <label className="switch-wrapper">
              <input
                type="checkbox"
                checked={exposeDeps}
                onChange={(e) => {
                  setExposeDeps(e.target.checked);
                  saveVal("yode-expose-deps", e.target.checked);
                }}
              />
              <span className="switch-slider" />
            </label>
          </div>

          <div className="divider" />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("诊断 Yode 工作区问题", "Diagnose issues in Yode Workspace")}</span>
              <span className="row-desc">{t("检查当前环境包并记录诊断日志", "Checks the current bundle and records diagnostic logs")}</span>
            </div>
            <button
              className="secondary-button"
              style={{ display: "flex", alignItems: "center", gap: "6px", paddingInline: "12px", height: "28px" }}
              type="button"
            >
              <Search size={12} />
              <span>{t("诊断", "Diagnose")}</span>
            </button>
          </div>

          <div className="divider" />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("重置并安装工作区", "Reset and install Workspace")}</span>
              <span className="row-desc">{t("删除本地缓存，重新下载并重新加载工具", "Deletes the local bundle, downloads it again, and reloads tools")}</span>
            </div>
            <button
              className="secondary-button"
              style={{
                color: "oklch(67% 0.15 28)",
                borderColor: "rgba(224, 80, 80, 0.2)",
                display: "flex",
                alignItems: "center",
                gap: "6px",
                paddingInline: "12px",
                height: "28px"
              }}
              type="button"
            >
              <Download size={12} />
              <span>{t("重新安装", "Reinstall")}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
