import React, { useEffect, useState } from "react";
import { Plus, Trash2, X, Settings } from "lucide-react";
import { CustomSelect } from "../CustomSelect";
import { loadDesktopSetting, saveDesktopSetting } from "../../lib/desktopSettings";

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
  const [statusText, setStatusText] = useState("");

  const saveServers = (newServers: McpServer[]) => {
    setServers(newServers);
    void saveDesktopSetting("yode-mcp-servers", newServers);
  };

  useEffect(() => {
    void loadDesktopSetting("yode-mcp-servers", servers).then(setServers);
  }, []);

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
      setStatusText(t("服务器名称不能为空。", "Server name cannot be empty."));
      return;
    }

    if (modalMode === "add" && servers.some((s) => s.name === formName.trim())) {
      setStatusText(t("服务器名称已存在。", "Server name already exists."));
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
    setStatusText(t("MCP 服务器配置已保存。", "MCP server configuration saved."));
    setIsModalOpen(false);
  };

  const handleDeleteServer = () => {
    if (!editingServer) return;
    if (confirm(t(`确定要删除服务器 "${editingServer.name}" 吗？`, `Are you sure you want to delete server "${editingServer.name}"?`))) {
      const updated = servers.filter((s) => s.name !== editingServer.name);
      saveServers(updated);
      setStatusText(t("MCP 服务器已删除。", "MCP server deleted."));
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
        {statusText && (
          <div style={{ fontSize: "11px", color: "var(--text-soft)" }}>
            {statusText}
          </div>
        )}
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
