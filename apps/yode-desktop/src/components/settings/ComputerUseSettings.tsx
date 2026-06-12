import React, { useState } from "react";
import { Plus, Trash2, X } from "lucide-react";

export function ComputerUseSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [anyAppStatus, setAnyAppStatus] = useState<"installed" | "uninstalled" | "installing">(() => {
    return (localStorage.getItem("yode-computer-use-anyapp") as any) || "uninstalled";
  });
  const [chromeStatus, setChromeStatus] = useState<"installed" | "uninstalled" | "installing">(() => {
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

  const saveAllowedApps = (list: string[]) => {
    setAllowedApps(list);
    localStorage.setItem("yode-computer-use-allowed-apps", JSON.stringify(list));
  };

  const handleInstallAnyApp = () => {
    if (anyAppStatus === "installed") {
      if (confirm(t("确定要卸载 Any App 权限吗？", "Are you sure you want to uninstall Any App access?"))) {
        setAnyAppStatus("uninstalled");
        localStorage.setItem("yode-computer-use-anyapp", "uninstalled");
      }
      return;
    }
    setAnyAppStatus("installing");
    setTimeout(() => {
      setAnyAppStatus("installed");
      localStorage.setItem("yode-computer-use-anyapp", "installed");
    }, 1200);
  };

  const handleInstallChrome = () => {
    if (chromeStatus === "installed") {
      if (confirm(t("确定要卸载 Google Chrome 扩展连接吗？", "Are you sure you want to disconnect Google Chrome extension?"))) {
        setChromeStatus("uninstalled");
        localStorage.setItem("yode-computer-use-chrome", "uninstalled");
      }
      return;
    }
    setChromeStatus("installing");
    setTimeout(() => {
      setChromeStatus("installed");
      localStorage.setItem("yode-computer-use-chrome", "installed");
    }, 1200);
  };

  const handleAddApp = () => {
    const name = newAppName.trim();
    if (!name) return;
    if (!allowedApps.includes(name)) {
      saveAllowedApps([...allowedApps, name]);
    }
    setNewAppName("");
    setShowAppModal(false);
  };

  const handleRemoveApp = (name: string) => {
    saveAllowedApps(allowedApps.filter((a) => a !== name));
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
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("始终允许的应用", "Always-allowed apps")}
          </span>
          <button
            onClick={() => setShowAppModal(true)}
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
                onClick={() => setShowAppModal(false)}
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
                onClick={() => setShowAppModal(false)}
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
