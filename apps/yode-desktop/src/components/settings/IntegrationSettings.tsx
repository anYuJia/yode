import React, { useState } from "react";
import { Plus, Trash2, X, Settings, Globe, Fingerprint } from "lucide-react";
import { CustomSelect } from "../CustomSelect";

// ----------------------------------------------------
// Computer Use Settings Component
// ----------------------------------------------------
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

// ----------------------------------------------------
// MCP Settings Component
// ----------------------------------------------------
interface McpServer {
  name: string;
  transport: "stdio" | "sse" | "http" | "websocket";
  command?: string;
  args?: string[];
  url?: string;
  env?: Record<string, string>;
  disabled: boolean;
}

export function McpSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [servers, setServers] = useState<McpServer[]>(() => {
    const saved = localStorage.getItem("yode-mcp-servers");
    if (saved) {
      try {
        return JSON.parse(saved);
      } catch (e) {
        // Fallback
      }
    }
    return [
      {
        name: "node_repl",
        transport: "stdio",
        command: "node",
        args: [],
        env: {},
        disabled: false
      }
    ];
  });

  const saveServers = (newServers: McpServer[]) => {
    setServers(newServers);
    localStorage.setItem("yode-mcp-servers", JSON.stringify(newServers));
  };

  const handleToggleServer = (name: string) => {
    const updated = servers.map((s) => (s.name === name ? { ...s, disabled: !s.disabled } : s));
    saveServers(updated);
  };

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [editingServer, setEditingServer] = useState<McpServer | null>(null);
  const [modalMode, setModalMode] = useState<"add" | "edit">("add");

  const [formName, setFormName] = useState("");
  const [formTransport, setFormTransport] = useState<"stdio" | "sse" | "http" | "websocket">("stdio");
  const [formCommand, setFormCommand] = useState("");
  const [formArgs, setFormArgs] = useState("");
  const [formUrl, setFormUrl] = useState("");
  const [formEnv, setFormEnv] = useState<Array<{ key: string; value: string }>>([]);

  const openAddModal = () => {
    setModalMode("add");
    setEditingServer(null);
    setFormName("");
    setFormTransport("stdio");
    setFormCommand("");
    setFormArgs("");
    setFormUrl("");
    setFormEnv([]);
    setIsModalOpen(true);
  };

  const openEditModal = (server: McpServer) => {
    setModalMode("edit");
    setEditingServer(server);
    setFormName(server.name);
    setFormTransport(server.transport);
    setFormCommand(server.command || "");
    setFormArgs((server.args || []).join(" "));
    setFormUrl(server.url || "");

    const envPairs = Object.entries(server.env || {}).map(([key, value]) => ({ key, value }));
    setFormEnv(envPairs);
    setIsModalOpen(true);
  };

  const handleAddEnv = () => {
    setFormEnv([...formEnv, { key: "", value: "" }]);
  };

  const handleRemoveEnv = (index: number) => {
    setFormEnv(formEnv.filter((_, i) => i !== index));
  };

  const handleEnvChange = (index: number, field: "key" | "value", val: string) => {
    const updated = [...formEnv];
    updated[index][field] = val;
    setFormEnv(updated);
  };

  const handleSave = () => {
    if (!formName.trim()) {
      alert(t("服务器名称不能为空", "Server name cannot be empty"));
      return;
    }

    if (modalMode === "add" && servers.some((s) => s.name === formName.trim())) {
      alert(t("服务器名称已存在", "Server name already exists"));
      return;
    }

    const envObj: Record<string, string> = {};
    formEnv.forEach((pair) => {
      if (pair.key.trim()) {
        envObj[pair.key.trim()] = pair.value;
      }
    });

    const parsedArgs = formArgs.trim() ? formArgs.trim().split(/\s+/) : [];

    const newServer: McpServer = {
      name: formName.trim(),
      transport: formTransport,
      disabled: editingServer ? editingServer.disabled : false,
      ...(formTransport === "stdio" ? { command: formCommand.trim(), args: parsedArgs, env: envObj } : {}),
      ...(formTransport !== "stdio" ? { url: formUrl.trim() } : {})
    };

    let updatedServers: McpServer[];
    if (modalMode === "add") {
      updatedServers = [...servers, newServer];
    } else {
      updatedServers = servers.map((s) => (s.name === editingServer?.name ? newServer : s));
    }

    saveServers(updatedServers);
    setIsModalOpen(false);
  };

  const handleDeleteServer = () => {
    if (!editingServer) return;
    if (confirm(t(`确定要删除服务器 "${editingServer.name}" 吗？`, `Are you sure you want to delete server "${editingServer.name}"?`))) {
      const updated = servers.filter((s) => s.name !== editingServer.name);
      saveServers(updated);
      setIsModalOpen(false);
    }
  };

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
        <p style={{ margin: 0, fontSize: "12px", color: "var(--text-soft)" }}>
          {t("连接外部工具和数据源。", "Connect external tools and data sources.")}{" "}
          <a href="#learn-mcp" style={{ color: "var(--accent)", textDecoration: "none" }}>
            {t("了解更多", "Learn more.")}
          </a>
        </p>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("服务器", "Servers")}
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
            <span>{t("添加服务器", "Add server")}</span>
          </button>
        </div>

        <div className="theme-card" style={{ padding: servers.length > 0 ? "8px 0" : "24px 16px" }}>
          {servers.length === 0 ? (
            <div style={{ textAlign: "center", color: "var(--text-soft)", fontSize: "13px" }}>
              {t("暂无配置的 MCP 服务器", "No configured MCP servers")}
            </div>
          ) : (
            servers.map((server, idx) => (
              <div key={server.name}>
                {idx > 0 && <div className="divider" style={{ margin: "4px 16px" }} />}
                <div className="form-row" style={{ minHeight: "48px" }}>
                  <div className="row-info" style={{ gap: "4px" }}>
                    <span className="row-label" style={{ fontFamily: "var(--font-code)", fontSize: "13.5px" }}>
                      {server.name}
                    </span>
                    <span className="row-desc" style={{ fontSize: "11px", color: "var(--text-soft)", opacity: 0.8 }}>
                      {server.transport === "stdio" ? `${server.command} ${(server.args || []).join(" ")}` : server.url}
                    </span>
                  </div>

                  <div style={{ display: "flex", alignItems: "center", gap: "16px" }}>
                    <button
                      onClick={() => openEditModal(server)}
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
                      <input type="checkbox" checked={!server.disabled} onChange={() => handleToggleServer(server.name)} />
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
              width: "480px",
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
                {modalMode === "add" ? t("添加 MCP 服务器", "Add MCP Server") : t("配置 MCP 服务器", "Configure MCP Server")}
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
                  {t("服务器名称", "Server Name")}
                </label>
                <input
                  type="text"
                  value={formName}
                  onChange={(e) => setFormName(e.target.value)}
                  disabled={modalMode === "edit"}
                  placeholder="e.g. node_repl"
                  style={{
                    background: "var(--field)",
                    border: "1px solid var(--line-soft)",
                    borderRadius: "var(--radius)",
                    padding: "8px 12px",
                    fontSize: "12.5px",
                    color: modalMode === "edit" ? "var(--text-soft)" : "var(--text)",
                    outline: "none",
                    fontFamily: "var(--font-code)",
                    opacity: modalMode === "edit" ? 0.7 : 1
                  }}
                />
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
                  {t("传输协议", "Transport Protocol")}
                </label>
                <CustomSelect
                  value={formTransport}
                  onChange={(val: any) => setFormTransport(val)}
                  options={[
                    { value: "stdio", label: "Stdio (Standard I/O)", avatarText: "🐚" },
                    { value: "sse", label: "SSE (Server-Sent Events)", avatarText: "📡" },
                    { value: "http", label: "HTTP (Standard API)", avatarText: "🌐" },
                    { value: "websocket", label: "Websocket (WS)", avatarText: "🔌" }
                  ]}
                  style={{ width: "100%" }}
                />
              </div>

              {formTransport === "stdio" && (
                <>
                  <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                    <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
                      {t("执行指令", "Command")}
                    </label>
                    <input
                      type="text"
                      value={formCommand}
                      onChange={(e) => setFormCommand(e.target.value)}
                      placeholder="e.g. node, python, npx"
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
                      {t("运行参数", "Arguments")}
                    </label>
                    <input
                      type="text"
                      value={formArgs}
                      onChange={(e) => setFormArgs(e.target.value)}
                      placeholder="e.g. path/to/server.js --arg1 value"
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

                  <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                      <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
                        {t("环境变量", "Environment Variables")}
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
                      {formEnv.map((pair, idx) => (
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
                      {formEnv.length === 0 && (
                        <span style={{ fontSize: "11px", color: "var(--text-soft)", opacity: 0.6, fontStyle: "italic" }}>
                          {t("无环境变量", "No environment variables")}
                        </span>
                      )}
                    </div>
                  </div>
                </>
              )}

              {formTransport !== "stdio" && (
                <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                  <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
                    {t("服务器 URL", "Server URL")}
                  </label>
                  <input
                    type="text"
                    value={formUrl}
                    onChange={(e) => setFormUrl(e.target.value)}
                    placeholder="e.g. http://localhost:3000/mcp"
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
              )}
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
                  onClick={handleDeleteServer}
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
// Browser Settings Component
// ----------------------------------------------------
export function BrowserSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [browserEnabled, setBrowserEnabled] = useState(() => {
    return localStorage.getItem("yode-browser-enabled") !== "false";
  });
  const [annotationScreenshots, setAnnotationScreenshots] = useState(() => {
    return localStorage.getItem("yode-browser-annotation-screenshots") || "Always include";
  });
  const [approvalPolicy, setApprovalPolicy] = useState(() => {
    return localStorage.getItem("yode-browser-approval") || "Always ask";
  });
  const [blockedDomains, setBlockedDomains] = useState<string[]>(() => {
    const saved = localStorage.getItem("yode-browser-blocked-domains");
    try {
      return saved ? JSON.parse(saved) : [];
    } catch (e) {
      return [];
    }
  });
  const [allowedDomains, setAllowedDomains] = useState<string[]>(() => {
    const saved = localStorage.getItem("yode-browser-allowed-domains");
    try {
      return saved ? JSON.parse(saved) : [];
    } catch (e) {
      return [];
    }
  });

  const [domainModalType, setDomainModalType] = useState<"blocked" | "allowed" | null>(null);
  const [newDomainInput, setNewDomainInput] = useState("");

  const saveBlocked = (list: string[]) => {
    setBlockedDomains(list);
    localStorage.setItem("yode-browser-blocked-domains", JSON.stringify(list));
  };

  const saveAllowed = (list: string[]) => {
    setAllowedDomains(list);
    localStorage.setItem("yode-browser-allowed-domains", JSON.stringify(list));
  };

  const handleAddDomain = () => {
    const domain = newDomainInput.trim().toLowerCase();
    if (!domain) return;
    if (domainModalType === "blocked") {
      if (!blockedDomains.includes(domain)) {
        saveBlocked([...blockedDomains, domain]);
      }
    } else if (domainModalType === "allowed") {
      if (!allowedDomains.includes(domain)) {
        saveAllowed([...allowedDomains, domain]);
      }
    }
    setNewDomainInput("");
    setDomainModalType(null);
  };

  const handleRemoveDomain = (type: "blocked" | "allowed", domain: string) => {
    if (type === "blocked") {
      saveBlocked(blockedDomains.filter((d) => d !== domain));
    } else {
      saveAllowed(allowedDomains.filter((d) => d !== domain));
    }
  };

  const handleClearBrowsingData = () => {
    alert(t("已成功清除应用内浏览器的所有数据与缓存！", "All data and cache from the in-app browser cleared successfully!"));
  };

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div style={{ fontSize: "12px", color: "var(--text-soft)", display: "flex", flexDirection: "column", gap: "2px" }}>
        <span>
          {t("管理 Yode 的浏览器。Google Chrome 可以在", "Manage Yode's browser. Google Chrome can be set up in")}{" "}
          <a href="#computer-use" style={{ color: "var(--accent)", textDecoration: "none" }}>
            {t("计算机使用设置", "computer use settings")}
          </a>{" "}
          {t("中进行配置。", "settings.")}
        </span>
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
              <Globe size={18} />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
              <span style={{ fontWeight: "650", fontSize: "13px", color: "var(--text)" }}>{t("浏览器", "Browser")}</span>
              <span style={{ fontSize: "11px", color: "var(--text-soft)" }}>{t("允许 Yode 控制内置浏览器", "Let Yode control the built-in browser")}</span>
            </div>
          </div>
          <label className="switch-wrapper">
            <input
              type="checkbox"
              checked={browserEnabled}
              onChange={(e) => {
                setBrowserEnabled(e.target.checked);
                localStorage.setItem("yode-browser-enabled", String(e.target.checked));
              }}
            />
            <span className="switch-slider" />
          </label>
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
          {t("数据管理", "Data")}
        </span>
        <div className="theme-card">
          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("浏览数据", "Browsing data")}</span>
              <span className="row-desc">{t("清除应用内浏览器的网站数据 and 缓存", "Clear site data and cache from the in-app browser")}</span>
            </div>
            <button
              onClick={handleClearBrowsingData}
              type="button"
              className="secondary-button"
              style={{ paddingInline: "12px", height: "26px", fontSize: "11.5px" }}
            >
              {t("清除所有浏览数据", "Clear all browsing data")}
            </button>
          </div>

          <div className="divider" />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("网页标注截图", "Annotation screenshots")}</span>
              <span className="row-desc">
                {t(
                  "截图能帮助 Yode 更好地理解和解决内容问题，但会增加 Token 用量",
                  "Screenshots help Yode better understand and address comments, but increase plan usage"
                )}
              </span>
            </div>
            <CustomSelect
              value={annotationScreenshots}
              onChange={(val) => {
                setAnnotationScreenshots(val);
                localStorage.setItem("yode-browser-annotation-screenshots", val);
              }}
              options={[
                { value: "Always include", label: t("总是包含", "Always include") },
                { value: "Never include", label: t("从不包含", "Never include") },
                { value: "Ask each time", label: t("每次询问", "Ask each time") }
              ]}
              style={{ minWidth: "150px" }}
            />
          </div>
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
          {t("权限与审批", "Permissions")}
        </span>
        <div className="theme-card">
          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("授权审批", "Approval")}</span>
              <span className="row-desc">
                {t("选择 Yode 在打开网站前是否需要征求您的同意。", "Choose if Yode asks for approval before opening websites.")}{" "}
                <a href="#learn-approval" style={{ color: "var(--accent)", textDecoration: "none" }}>
                  {t("了解更多", "Learn more")}
                </a>
              </span>
            </div>
            <CustomSelect
              value={approvalPolicy}
              onChange={(val) => {
                setApprovalPolicy(val);
                localStorage.setItem("yode-browser-approval", val);
              }}
              options={[
                { value: "Always ask", label: t("总是询问", "Always ask") },
                { value: "Always allow", label: t("总是允许", "Always allow") },
                { value: "Never allow", label: t("从不允许", "Never allow") }
              ]}
              style={{ minWidth: "150px" }}
            />
          </div>
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("已拦截域名", "Blocked domains")}
          </span>
          <button
            onClick={() => setDomainModalType("blocked")}
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
        <span style={{ fontSize: "11px", color: "var(--text-soft)", marginTop: "-4px" }}>
          {t("Yode 将永远不会打开这些网站", "Yode will never open these sites")}
        </span>
        <div className="theme-card" style={{ padding: blockedDomains.length > 0 ? "4px 0" : "16px 12px" }}>
          {blockedDomains.length === 0 ? (
            <div style={{ textAlign: "center", fontSize: "12px", color: "var(--text-soft)", opacity: 0.7 }}>
              {t("暂无被拦截的域名", "No blocked domains")}
            </div>
          ) : (
            blockedDomains.map((domain, idx) => (
              <div key={domain}>
                {idx > 0 && <div className="divider" style={{ margin: "2px 16px" }} />}
                <div className="form-row" style={{ minHeight: "36px", paddingBlock: "4px" }}>
                  <span style={{ fontFamily: "var(--font-code)", fontSize: "12.5px" }}>{domain}</span>
                  <button
                    onClick={() => handleRemoveDomain("blocked", domain)}
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

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("已允许域名", "Allowed domains")}
          </span>
          <button
            onClick={() => setDomainModalType("allowed")}
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
        <span style={{ fontSize: "11px", color: "var(--text-soft)", marginTop: "-4px" }}>
          {t("无需询问即可直接打开的域名", "Domains that open without asking")}
        </span>
        <div className="theme-card" style={{ padding: allowedDomains.length > 0 ? "4px 0" : "16px 12px" }}>
          {allowedDomains.length === 0 ? (
            <div style={{ textAlign: "center", fontSize: "12px", color: "var(--text-soft)", opacity: 0.7 }}>
              {t("暂无自动允许的域名", "No allowed domains")}
            </div>
          ) : (
            allowedDomains.map((domain, idx) => (
              <div key={domain}>
                {idx > 0 && <div className="divider" style={{ margin: "2px 16px" }} />}
                <div className="form-row" style={{ minHeight: "36px", paddingBlock: "4px" }}>
                  <span style={{ fontFamily: "var(--font-code)", fontSize: "12.5px" }}>{domain}</span>
                  <button
                    onClick={() => handleRemoveDomain("allowed", domain)}
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

      {domainModalType !== null && (
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
                {domainModalType === "blocked" ? t("拦截新域名", "Block Domain") : t("允许新域名", "Allow Domain")}
              </span>
              <button
                onClick={() => setDomainModalType(null)}
                type="button"
                style={{ background: "transparent", border: "none", cursor: "pointer", color: "var(--text-soft)", display: "flex" }}
              >
                <X size={14} />
              </button>
            </div>

            <div style={{ padding: "16px", display: "flex", flexDirection: "column", gap: "12px" }}>
              <input
                type="text"
                value={newDomainInput}
                onChange={(e) => setNewDomainInput(e.target.value)}
                placeholder="e.g. example.com"
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
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleAddDomain();
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
                onClick={() => setDomainModalType(null)}
                type="button"
                style={{ background: "transparent", border: "none", fontSize: "12px", color: "var(--text-soft)", cursor: "pointer" }}
              >
                {t("取消", "Cancel")}
              </button>
              <button
                onClick={handleAddDomain}
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
