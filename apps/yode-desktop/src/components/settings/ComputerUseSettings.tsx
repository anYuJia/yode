import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen, Plus, Trash2, X } from "lucide-react";
import { isTauriRuntime, loadDesktopSetting } from "../../lib/desktopSettings";

type InstallStatus = "installed" | "uninstalled" | "installing";

type ComputerUseSettingsState = {
  anyAppStatus: InstallStatus;
  chromeStatus: InstallStatus;
  allowedApps: string[];
};

const DEFAULT_COMPUTER_USE_SETTINGS: ComputerUseSettingsState = {
  anyAppStatus: "uninstalled",
  chromeStatus: "uninstalled",
  allowedApps: []
};

function persistComputerUseFallback(settings: ComputerUseSettingsState) {
  localStorage.setItem("yode-computer-use-anyapp", settings.anyAppStatus);
  localStorage.setItem("yode-computer-use-chrome", settings.chromeStatus);
  localStorage.setItem("yode-computer-use-allowed-apps", JSON.stringify(settings.allowedApps));
}

function normalizeAppName(value: string): string | null {
  const name = value.trim().replace(/\.app$/i, "").trim();
  if (!name || name.length > 80 || name.includes("\0")) return null;
  return name;
}

export function ComputerUseSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [anyAppStatus, setAnyAppStatus] = useState<InstallStatus>(() => {
    return (localStorage.getItem("yode-computer-use-anyapp") as any) || "uninstalled";
  });
  const [chromeStatus, setChromeStatus] = useState<InstallStatus>(() => {
    return (localStorage.getItem("yode-computer-use-chrome") as any) || "uninstalled";
  });
  const [allowedApps, setAllowedApps] = useState<string[]>(() => {
    const saved = localStorage.getItem("yode-computer-use-allowed-apps");
    try {
      return saved ? JSON.parse(saved) : [];
    } catch (e) {
      return [];
    }
  });

  const [showAppModal, setShowAppModal] = useState(false);
  const [newAppName, setNewAppName] = useState("");
  const [statusText, setStatusText] = useState("");
  const [appError, setAppError] = useState("");

  useEffect(() => {
    const loadSettings = async () => {
      if (isTauriRuntime()) {
        try {
          const settings = await invoke<ComputerUseSettingsState>("computer_use_settings_get");
          applySettingsToState(settings);
          setStatusText(t("计算机使用设置已连接到运行时。", "Computer use settings are connected to the runtime."));
          return;
        } catch (err) {
          console.error(err);
        }
      }

      const fallback = {
        anyAppStatus: await loadDesktopSetting(
          "yode-computer-use-anyapp",
          DEFAULT_COMPUTER_USE_SETTINGS.anyAppStatus
        ),
        chromeStatus: await loadDesktopSetting(
          "yode-computer-use-chrome",
          DEFAULT_COMPUTER_USE_SETTINGS.chromeStatus
        ),
        allowedApps: await loadDesktopSetting(
          "yode-computer-use-allowed-apps",
          DEFAULT_COMPUTER_USE_SETTINGS.allowedApps
        )
      };
      applySettingsToState(fallback as ComputerUseSettingsState);
    };
    void loadSettings();
  }, []);

  const currentSettings = (): ComputerUseSettingsState => ({
    anyAppStatus,
    chromeStatus,
    allowedApps
  });

  const applySettingsToState = (settings: ComputerUseSettingsState) => {
    setAnyAppStatus(settings.anyAppStatus);
    setChromeStatus(settings.chromeStatus);
    setAllowedApps(settings.allowedApps);
  };

  const applyComputerUseSettings = async (nextSettings: ComputerUseSettingsState) => {
    try {
      if (isTauriRuntime()) {
        const applied = await invoke<ComputerUseSettingsState>("computer_use_settings_apply", { settings: nextSettings });
        applySettingsToState(applied);
      } else {
        persistComputerUseFallback(nextSettings);
        applySettingsToState(nextSettings);
      }
      setStatusText(t("计算机使用设置已应用。", "Computer use settings applied."));
    } catch (err) {
      console.error(err);
      setStatusText(t("应用计算机使用设置失败。", "Failed to apply computer use settings."));
    }
  };

  const handleInstallAnyApp = async () => {
    if (anyAppStatus === "installed") {
      if (confirm(t("确定要卸载 Any App 权限吗？", "Are you sure you want to uninstall Any App access?"))) {
        await applyComputerUseSettings({ ...currentSettings(), anyAppStatus: "uninstalled" });
      }
      return;
    }
    setAnyAppStatus("installing");
    if (isTauriRuntime()) {
      const result = await invoke<{ message: string }>("computer_use_open_accessibility").catch((err) => {
        console.error(err);
        return { message: t("打开系统权限设置失败。", "Failed to open system permissions.") };
      });
      setStatusText(result.message);
    }
    await applyComputerUseSettings({ ...currentSettings(), anyAppStatus: "installed" });
  };

  const handleInstallChrome = async () => {
    if (chromeStatus === "installed") {
      if (confirm(t("确定要卸载 Google Chrome 扩展连接吗？", "Are you sure you want to disconnect Google Chrome extension?"))) {
        await applyComputerUseSettings({ ...currentSettings(), chromeStatus: "uninstalled" });
      }
      return;
    }
    setChromeStatus("installing");
    if (isTauriRuntime()) {
      const result = await invoke<{ ok: boolean; message: string }>("computer_use_open_chrome").catch((err) => {
        console.error(err);
        return { ok: false, message: t("打开 Google Chrome 失败。", "Failed to open Google Chrome.") };
      });
      setStatusText(result.message);
      await applyComputerUseSettings({ ...currentSettings(), chromeStatus: result.ok ? "installed" : "uninstalled" });
      return;
    }
    await applyComputerUseSettings({ ...currentSettings(), chromeStatus: "installed" });
  };

  const handleAddApp = () => {
    const name = normalizeAppName(newAppName);
    if (!name) {
      setAppError(t("请输入有效应用名称。", "Enter a valid application name."));
      return;
    }
    setAppError("");
    const exists = allowedApps.some((app) => app.toLowerCase() === name.toLowerCase());
    const nextApps = exists ? allowedApps : [...allowedApps, name];
    void applyComputerUseSettings({ ...currentSettings(), allowedApps: nextApps });
    setNewAppName("");
    setShowAppModal(false);
  };

  const handlePickApp = async () => {
    if (!isTauriRuntime()) {
      setStatusText(t("桌面端可使用系统选择器添加应用。", "Use the desktop runtime to pick an app."));
      return;
    }
    const result = await invoke<{ ok: boolean; message: string; path?: string | null }>("computer_use_pick_application").catch((err) => {
      console.error(err);
      return { ok: false, message: t("选择应用失败。", "Failed to pick application.") };
    });
    if (!result.ok) {
      setStatusText(result.message);
      return;
    }
    const name = normalizeAppName(result.message);
    if (!name) return;
    const exists = allowedApps.some((app) => app.toLowerCase() === name.toLowerCase());
    const nextApps = exists ? allowedApps : [...allowedApps, name];
    await applyComputerUseSettings({ ...currentSettings(), allowedApps: nextApps });
    setStatusText(t(`已添加 ${name}。`, `Added ${name}.`));
  };

  const handleRemoveApp = (name: string) => {
    void applyComputerUseSettings({ ...currentSettings(), allowedApps: allowedApps.filter((a) => a !== name) });
  };

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div style={{ fontSize: "12px", color: "var(--text-soft)" }}>
        {t("管理 Yode 如何使用您计算机上的其他应用程序。", "Manage how Yode uses other applications on your computer.")}
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
          {t("控制权限", "Control")}
        </span>
        <div className="theme-card" style={{ padding: "4px 0" }}>
          <div className="form-row" style={{ minHeight: "56px" }}>
            <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
              <div
                style={{
                  width: "36px",
                  height: "36px",
                  borderRadius: "8px",
                  background: "linear-gradient(135deg, #FF6B6B 0%, #4D96FF 100%)",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  boxShadow: "0 2px 8px rgba(0,0,0,0.2)"
                }}
              >
                <span style={{ color: "#FFF", fontSize: "16px", fontWeight: "bold" }}>A</span>
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
                <span style={{ fontWeight: "650", fontSize: "13px", color: "var(--text)" }}>{t("Any App", "Any App")}</span>
                <span style={{ fontSize: "11px", color: "var(--text-soft)" }}>
                  {t("允许 Yode 控制您计算机上的应用", "Let Yode control apps on your computer")}
                </span>
              </div>
            </div>

            <button
              onClick={handleInstallAnyApp}
              disabled={anyAppStatus === "installing"}
              type="button"
              className="secondary-button"
              style={{
                paddingInline: "16px",
                height: "26px",
                fontSize: "11.5px",
                background: anyAppStatus === "installed" ? "var(--field)" : "var(--accent)",
                color: anyAppStatus === "installed" ? "var(--text)" : "var(--bg)",
                border: "none",
                fontWeight: "600",
                minWidth: "72px"
              }}
            >
              {anyAppStatus === "installing"
                ? t("正在安装...", "Installing...")
                : anyAppStatus === "installed"
                ? t("卸载", "Uninstall")
                : t("安装", "Install")}
            </button>
          </div>

          <div className="divider" />

          <div className="form-row" style={{ minHeight: "56px" }}>
            <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
              <div
                style={{
                  width: "36px",
                  height: "36px",
                  borderRadius: "8px",
                  background: "var(--field)",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  boxShadow: "0 2px 8px rgba(0,0,0,0.15)"
                }}
              >
                <svg width="20" height="20" viewBox="0 0 24 24">
                  <path fill="#4285F4" d="M12 0C8.21 0 4.89 1.77 2.76 4.53L7.75 13.17C8.19 11.23 9.93 9.77 12 9.77H22.9C21.84 4.07 17.39 0 12 0z" />
                  <path fill="#EA4335" d="M22.9 9.77H12.01C10.74 9.77 9.57 10.42 8.89 11.45L3.45 20.87C6.01 22.8 9.21 24 12.01 24C16.89 24 21.05 20.73 22.9 16.23L22.9 9.77z" />
                  <path fill="#FBBC05" d="M8.89 11.45C8.21 12.48 8 13.72 8.35 14.93L2.76 4.53C1.04 6.75 0 9.53 0 12.5C0 17.9 3.56 22.42 8.89 23.97L8.89 11.45z" />
                  <circle fill="#FFFFFF" cx="12" cy="12" r="5" />
                  <circle fill="#4285F4" cx="12" cy="12" r="3.5" />
                </svg>
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
                <span style={{ fontWeight: "650", fontSize: "13px", color: "var(--text)" }}>{t("Google Chrome", "Google Chrome")}</span>
                <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                  <div
                    style={{
                      width: "6px",
                      height: "6px",
                      borderRadius: "50%",
                      background: chromeStatus === "installed" ? "var(--success, #50FA7B)" : "var(--error, #FF5555)"
                    }}
                  />
                  <span style={{ fontSize: "11px", color: "var(--text-soft)" }}>
                    {chromeStatus === "installed"
                      ? t("浏览器扩展已连接", "Browser extension connected")
                      : t("浏览器扩展未连接", "Browser extension not connected")}
                  </span>
                </div>
              </div>
            </div>

            <button
              onClick={handleInstallChrome}
              disabled={chromeStatus === "installing"}
              type="button"
              className="secondary-button"
              style={{
                paddingInline: "16px",
                height: "26px",
                fontSize: "11.5px",
                background: chromeStatus === "installed" ? "var(--field)" : "var(--accent)",
                color: chromeStatus === "installed" ? "var(--text)" : "var(--bg)",
                border: "none",
                fontWeight: "600",
                minWidth: "72px"
              }}
            >
              {chromeStatus === "installing"
                ? t("正在连接...", "Connecting...")
                : chromeStatus === "installed"
                ? t("断开", "Disconnect")
                : t("安装", "Install")}
            </button>
          </div>
        </div>
        {statusText && (
          <div style={{ fontSize: "11px", color: "var(--text-soft)" }}>
            {statusText}
          </div>
        )}
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("始终允许的应用", "Always-allowed apps")}
          </span>
          <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
            <button
              onClick={() => {
                setAppError("");
                setShowAppModal(true);
              }}
              type="button"
              className="secondary-button"
              style={{
                display: "flex",
                alignItems: "center",
                gap: "4px",
                paddingInline: "10px",
                height: "22px",
                fontSize: "11px"
              }}
            >
              <Plus size={12} />
              <span>{t("添加", "Add")}</span>
            </button>
            {isTauriRuntime() && (
              <button
                onClick={handlePickApp}
                type="button"
                className="secondary-button"
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "4px",
                  paddingInline: "10px",
                  height: "22px",
                  fontSize: "11px"
                }}
              >
                <FolderOpen size={12} />
                <span>{t("选择", "Choose")}</span>
              </button>
            )}
          </div>
        </div>

        <div className="theme-card" style={{ padding: allowedApps.length > 0 ? "4px 0" : "16px 12px" }}>
          {allowedApps.length === 0 ? (
            <div style={{ textAlign: "center", fontSize: "12px", color: "var(--text-soft)", opacity: 0.7 }}>
              {t("暂无", "None yet")}
            </div>
          ) : (
            allowedApps.map((app, idx) => (
              <div key={app}>
                {idx > 0 && <div className="divider" style={{ margin: "2px 16px" }} />}
                <div className="form-row" style={{ minHeight: "36px", paddingBlock: "4px" }}>
                  <span style={{ fontWeight: "600", fontSize: "12.5px" }}>{app}</span>
                  <button
                    onClick={() => handleRemoveApp(app)}
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
                    <Trash2 size={13} />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>

      {showAppModal && (
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
              width: "360px",
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
              <span style={{ fontWeight: "600", fontSize: "13px", color: "var(--text)" }}>
                {t("添加始终允许的应用", "Add Always-Allowed App")}
              </span>
              <button
                onClick={() => {
                  setAppError("");
                  setShowAppModal(false);
                }}
                type="button"
                style={{ background: "transparent", border: "none", cursor: "pointer", color: "var(--text-soft)", display: "flex" }}
              >
                <X size={14} />
              </button>
            </div>

            <div style={{ padding: "16px", display: "flex", flexDirection: "column", gap: "12px" }}>
              <input
                type="text"
                value={newAppName}
                onChange={(e) => setNewAppName(e.target.value)}
                placeholder="e.g. Slack, VS Code, Finder"
                style={{
                  background: "var(--field)",
                  border: "1px solid var(--line-soft)",
                  borderRadius: "var(--radius)",
                  padding: "8px 12px",
                  fontSize: "12px",
                  color: "var(--text)",
                  outline: "none"
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleAddApp();
                }}
              />
              {appError && (
                <span style={{ fontSize: "11px", color: "oklch(67% 0.15 28)" }}>
                  {appError}
                </span>
              )}
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
                onClick={() => {
                  setAppError("");
                  setShowAppModal(false);
                }}
                type="button"
                style={{ background: "transparent", border: "none", fontSize: "12px", color: "var(--text-soft)", cursor: "pointer" }}
              >
                {t("取消", "Cancel")}
              </button>
              <button
                onClick={handleAddApp}
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
