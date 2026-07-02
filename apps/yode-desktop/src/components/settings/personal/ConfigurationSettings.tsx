import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Search, Download } from "lucide-react";
import { CustomSelect } from "../../CustomSelect";
import { Bootstrap } from "../../../lib/desktopTypes";
import { loadConfigurationSettings, saveConfigurationSettings } from "../../../lib/desktopSettings";

type ConfigurationState = {
  scope: string;
  approvalPolicy: string;
  sandboxSettings: string;
  exposeDependencies: boolean;
  configPath: string;
  projectConfigPath: string;
};

type DiagnosticCheck = {
  name: string;
  status: string;
  detail: string;
};

type WorkspaceDiagnosticsResult = {
  reportPath: string;
  checks: DiagnosticCheck[];
};

export function ConfigurationSettings({ bootstrap, isZh, t }: { bootstrap: Bootstrap; isZh: boolean; t: (zh: string, en: string) => string }) {
  const initialConfiguration = loadConfigurationSettings();
  const [configScope, setConfigScope] = useState(initialConfiguration.scope);
  const [approvalPolicy, setApprovalPolicy] = useState(initialConfiguration.approvalPolicy);
  const [sandboxSettings, setSandboxSettings] = useState(initialConfiguration.sandboxSettings);
  const [exposeDeps, setExposeDeps] = useState(initialConfiguration.exposeDependencies);
  const [configPath, setConfigPath] = useState("");
  const [projectConfigPath, setProjectConfigPath] = useState("");
  const [statusText, setStatusText] = useState("");
  const [diagnostics, setDiagnostics] = useState<WorkspaceDiagnosticsResult | null>(null);
  const [busy, setBusy] = useState<"diagnose" | "reinstall" | "save" | null>(null);

  const applyConfiguration = async (next?: Partial<ConfigurationState>) => {
    const request = {
      scope: next?.scope ?? configScope,
      approvalPolicy: next?.approvalPolicy ?? approvalPolicy,
      sandboxSettings: next?.sandboxSettings ?? sandboxSettings,
      exposeDependencies: next?.exposeDependencies ?? exposeDeps
    };
    saveConfigurationSettings(request);
    if (!("__TAURI_INTERNALS__" in window)) return;
    setBusy("save");
    try {
      const state = await invoke<ConfigurationState>("configuration_update", { request });
      setConfigPath(state.configPath);
      setProjectConfigPath(state.projectConfigPath);
      setStatusText(t("配置已写入 config.toml，并同步到当前运行时。", "Configuration was written to config.toml and synced to the current runtime."));
    } catch (err) {
      console.error(err);
      setStatusText(t("保存配置失败。", "Failed to save configuration."));
    } finally {
      setBusy(null);
    }
  };

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    invoke<ConfigurationState>("configuration_state_get")
      .then((state) => {
        setConfigScope(state.scope);
        setApprovalPolicy(state.approvalPolicy);
        setSandboxSettings(state.sandboxSettings);
        setExposeDeps(state.exposeDependencies);
        setConfigPath(state.configPath);
        setProjectConfigPath(state.projectConfigPath);
        saveConfigurationSettings(state);
      })
      .catch(console.error);
  }, []);

  const openConfigFile = async () => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    try {
      await invoke("configuration_open_file", { scope: configScope });
      setStatusText(t("已用默认打开目标打开 config.toml。", "Opened config.toml with the default destination."));
    } catch (err) {
      console.error(err);
      setStatusText(t("打开 config.toml 失败。", "Failed to open config.toml."));
    }
  };

  const runDiagnostics = async () => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    setBusy("diagnose");
    setStatusText(t("正在诊断工作区...", "Diagnosing workspace..."));
    try {
      const result = await invoke<WorkspaceDiagnosticsResult>("workspace_diagnose");
      setDiagnostics(result);
      setStatusText(t(`诊断完成：${result.reportPath}`, `Diagnostics complete: ${result.reportPath}`));
    } catch (err) {
      console.error(err);
      setStatusText(t("诊断失败。", "Diagnostics failed."));
    } finally {
      setBusy(null);
    }
  };

  const reinstallWorkspace = async () => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    setBusy("reinstall");
    setStatusText(t("正在重置并安装工作区...", "Resetting and installing workspace..."));
    try {
      const result = await invoke<WorkspaceDiagnosticsResult>("workspace_reinstall");
      setDiagnostics(result);
      setExposeDeps(true);
      saveConfigurationSettings({
        scope: configScope,
        approvalPolicy,
        sandboxSettings,
        exposeDependencies: true
      });
      setStatusText(t(`工作区已重装：${result.reportPath}`, `Workspace reinstalled: ${result.reportPath}`));
    } catch (err) {
      console.error(err);
      setStatusText(t("重装工作区失败。", "Failed to reinstall workspace."));
    } finally {
      setBusy(null);
    }
  };

  const activeConfigPath = configScope === "Project config" ? projectConfigPath : configPath;

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
                void applyConfiguration({ scope: val });
              }}
              options={[
                { value: "User config", label: t("用户配置", "User config"), avatarText: "👤" },
                { value: "Project config", label: t("项目配置", "Project config"), avatarText: "📁" }
              ]}
              style={{ minWidth: "150px" }}
            />
            <button
              type="button"
              onClick={openConfigFile}
              style={{ fontSize: "11px", color: "var(--text-soft)", textDecoration: "none" }}
              className="hover-link"
            >
              {t("打开 config.toml ↗", "Open config.toml ↗")}
            </button>
          </div>
          {activeConfigPath && (
            <div style={{ fontSize: "11px", color: "var(--text-soft)", fontFamily: "var(--font-code)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {activeConfigPath}
            </div>
          )}

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
                void applyConfiguration({ approvalPolicy: val });
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
                void applyConfiguration({ sandboxSettings: val });
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
              {bootstrap.appVersion}
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
                  void applyConfiguration({ exposeDependencies: e.target.checked });
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
              disabled={busy === "diagnose"}
              onClick={runDiagnostics}
            >
              <Search size={12} />
              <span>{busy === "diagnose" ? t("诊断中", "Diagnosing") : t("诊断", "Diagnose")}</span>
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
              disabled={busy === "reinstall"}
              onClick={reinstallWorkspace}
            >
              <Download size={12} />
              <span>{busy === "reinstall" ? t("安装中", "Installing") : t("重新安装", "Reinstall")}</span>
            </button>
          </div>
        </div>
        {statusText && (
          <div style={{ fontSize: "11px", color: "var(--text-soft)", lineHeight: 1.5 }}>
            {statusText}
          </div>
        )}
        {diagnostics && (
          <div className="theme-card" style={{ padding: "12px", display: "flex", flexDirection: "column", gap: "8px" }}>
            {diagnostics.checks.map((check) => (
              <div key={check.name} style={{ display: "grid", gridTemplateColumns: "96px 70px 1fr", gap: "10px", alignItems: "center", fontSize: "11.5px" }}>
                <strong style={{ color: "var(--text)" }}>{check.name}</strong>
                <span style={{ color: check.status === "ok" ? "var(--success)" : check.status === "warn" ? "var(--warning)" : "var(--error)", fontFamily: "var(--font-code)" }}>
                  {check.status}
                </span>
                <span style={{ color: "var(--text-soft)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{check.detail}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
