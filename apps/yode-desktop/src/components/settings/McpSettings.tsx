import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, X, Settings, RotateCw, Activity } from "lucide-react";
import { CustomSelect } from "../CustomSelect";
import {
  isMcpTransport,
  isTauriRuntime,
  loadMcpServers,
  loadPersistedMcpServers,
  McpServer,
  normalizeMcpServers,
  saveMcpServers,
  savePersistedMcpServers
} from "../../lib/desktopSettings";

interface McpServerStatus {
  name: string;
  state: "configured" | "ready" | "failed" | "disabled" | string;
  detail: string;
  toolCount: number;
  resourceCount: number;
  templateCount: number;
}

interface McpState {
  configPath: string;
  servers: McpServer[];
  statuses: McpServerStatus[];
}

export function McpSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const initialServers = loadMcpServers();
  const [servers, setServers] = useState<McpServer[]>(initialServers);
  const [statusText, setStatusText] = useState("");
  const [configPath, setConfigPath] = useState("");
  const [serverStatuses, setServerStatuses] = useState<Record<string, McpServerStatus>>({});
  const [testingServer, setTestingServer] = useState<string | null>(null);
  const [deleteConfirmName, setDeleteConfirmName] = useState<string | null>(null);

  const applyMcpState = (state: McpState) => {
    const nextServers = normalizeMcpServers(state.servers);
    setServers(nextServers);
    setConfigPath(state.configPath);
    setServerStatuses(Object.fromEntries(state.statuses.map((status) => [status.name, status])));
    saveMcpServers(nextServers);
  };

  const saveServers = async (newServers: McpServer[]) => {
    const normalized = normalizeMcpServers(newServers);
    setServers(normalized);
    saveMcpServers(normalized);
    if (isTauriRuntime()) {
      const state = await invoke<McpState>("mcp_servers_save", { servers: normalized });
      applyMcpState(state);
    } else {
      await savePersistedMcpServers(normalized);
    }
  };

  useEffect(() => {
    if (isTauriRuntime()) {
      void invoke<McpState>("mcp_servers_state")
        .then(applyMcpState)
        .catch((err) => setStatusText(String(err)));
      return;
    }
    void loadPersistedMcpServers(initialServers).then(setServers);
  }, []);

  const handleToggleServer = (name: string) => {
    const updated = servers.map((s) => (s.name === name ? { ...s, disabled: !s.disabled } : s));
    void saveServers(updated).then(() => {
      setStatusText(t("MCP 配置已保存并重载。", "MCP configuration saved and reloaded."));
    }).catch((err) => setStatusText(String(err)));
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

    const parsedArgs = parseArgs(formArgs);

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

    void saveServers(updatedServers)
      .then(() => {
        setStatusText(t("MCP 服务器配置已保存并重载。", "MCP server configuration saved and reloaded."));
        setIsModalOpen(false);
      })
      .catch((err) => setStatusText(String(err)));
  };

  const handleDeleteServer = () => {
    if (!editingServer) return;
    if (deleteConfirmName !== editingServer.name) {
      setDeleteConfirmName(editingServer.name);
      return;
    }
    const updated = servers.filter((s) => s.name !== editingServer.name);
    void saveServers(updated)
      .then(() => {
        setStatusText(t("MCP 服务器已删除并重载。", "MCP server deleted and reloaded."));
        setIsModalOpen(false);
        setDeleteConfirmName(null);
      })
      .catch((err) => setStatusText(String(err)));
  };

  const handleTestServer = async (server: McpServer) => {
    setTestingServer(server.name);
    setStatusText(t(`正在测试 ${server.name}...`, `Testing ${server.name}...`));
    try {
      const status = isTauriRuntime()
        ? await invoke<McpServerStatus>("mcp_server_test", { server })
        : {
            name: server.name,
            state: server.disabled ? "disabled" : "configured",
            detail: t("浏览器预览中无法连接 MCP 服务器。", "MCP connection tests require the desktop runtime."),
            toolCount: 0,
            resourceCount: 0,
            templateCount: 0
          };
      setServerStatuses((current) => ({ ...current, [server.name]: status }));
      setStatusText(status.detail);
    } catch (err) {
      setStatusText(String(err));
    } finally {
      setTestingServer(null);
    }
  };

  const handleReload = async () => {
    if (!isTauriRuntime()) {
      setStatusText(t("浏览器预览中无法重载 MCP 运行时。", "MCP reload requires the desktop runtime."));
      return;
    }
    try {
      const state = await invoke<McpState>("mcp_servers_reload");
      applyMcpState(state);
      setStatusText(t("MCP 运行时已重载。", "MCP runtime reloaded."));
    } catch (err) {
      setStatusText(String(err));
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
        {configPath && (
          <div style={{ fontSize: "11px", color: "var(--text-soft)" }}>
            {t("配置文件", "Config file")} · <span style={{ fontFamily: "var(--font-code)" }}>{configPath}</span>
          </div>
        )}

        <div className="theme-card" style={{ padding: servers.length > 0 ? "8px 0" : "24px 16px" }}>
          {servers.length === 0 ? (
            <div style={{ textAlign: "center", color: "var(--text-soft)", fontSize: "13px" }}>
              {t("暂无配置的 MCP 服务器", "No configured MCP servers")}
            </div>
          ) : (
            servers.map((server, idx) => {
              const serverStatus = serverStatuses[server.name] ?? {
                name: server.name,
                state: server.disabled ? "disabled" : "configured",
                detail: server.disabled ? t("服务器已禁用。", "Server disabled.") : t("已保存到配置。", "Saved to config."),
                toolCount: 0,
                resourceCount: 0,
                templateCount: 0
              };
              return (
              <div key={server.name}>
                {idx > 0 && <div className="divider" style={{ margin: "4px 16px" }} />}
                <div className="form-row" style={{ minHeight: "64px", alignItems: "center" }}>
                  <div className="row-info" style={{ gap: "4px" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                      <span className="row-label" style={{ fontFamily: "var(--font-code)", fontSize: "13.5px" }}>
                        {server.name}
                      </span>
                      <StatusPill status={serverStatus} t={t} />
                    </div>
                    <span className="row-desc" style={{ fontSize: "11px", color: "var(--text-soft)", opacity: 0.8 }}>
                      {server.transport === "stdio" ? `${server.command} ${(server.args || []).join(" ")}` : server.url}
                    </span>
                    <span className="row-desc" style={{ fontSize: "11px", color: "var(--text-soft)", opacity: 0.75 }}>
                      {serverStatus.detail}
                    </span>
                  </div>

                  <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
                    <span style={{ fontSize: "11px", color: "var(--text-soft)", whiteSpace: "nowrap" }}>
                      {serverStatus.toolCount} tools · {serverStatus.resourceCount + serverStatus.templateCount} resources
                    </span>
                    <button
                      onClick={() => void handleTestServer(server)}
                      type="button"
                      className="secondary-button"
                      disabled={testingServer === server.name}
                      style={{ height: "26px", gap: "6px", paddingInline: "10px" }}
                    >
                      <Activity size={13} />
                      {testingServer === server.name ? t("测试中", "Testing") : t("测试", "Test")}
                    </button>
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
            )})
          )}
        </div>
        <button
          className="secondary-button"
          type="button"
          onClick={() => void handleReload()}
          style={{ alignSelf: "flex-end", height: "28px", gap: "6px" }}
        >
          <RotateCw size={13} />
          {t("重载 MCP", "Reload MCP")}
        </button>
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
                  onChange={(value) => {
                    if (isMcpTransport(value)) setFormTransport(value);
                  }}
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
                  {deleteConfirmName === editingServer?.name && (
                    <span style={{ opacity: 0.8 }}>{t("再次点击确认", "Click again")}</span>
                  )}
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

function StatusPill({
  status,
  t
}: {
  status: McpServerStatus;
  t: (zh: string, en: string) => string;
}) {
  const tone =
    status.state === "ready"
      ? { color: "oklch(62% 0.12 155)", bg: "rgba(42, 157, 96, 0.12)", label: t("就绪", "Ready") }
      : status.state === "failed"
        ? { color: "oklch(67% 0.15 28)", bg: "rgba(224, 80, 80, 0.12)", label: t("失败", "Failed") }
        : status.state === "disabled"
          ? { color: "var(--text-soft)", bg: "var(--field)", label: t("已停用", "Disabled") }
          : { color: "var(--accent)", bg: "color-mix(in oklab, var(--accent) 12%, transparent)", label: t("已配置", "Configured") };

  return (
    <span
      title={status.detail}
      style={{
        display: "inline-flex",
        alignItems: "center",
        height: "20px",
        paddingInline: "7px",
        borderRadius: "999px",
        background: tone.bg,
        color: tone.color,
        fontSize: "10.5px",
        fontWeight: 700
      }}
    >
      {tone.label}
    </span>
  );
}

function parseArgs(input: string): string[] {
  const args: string[] = [];
  let current = "";
  let quote: '"' | "'" | null = null;
  let escaping = false;

  for (const char of input.trim()) {
    if (escaping) {
      current += char;
      escaping = false;
      continue;
    }
    if (char === "\\") {
      escaping = true;
      continue;
    }
    if ((char === '"' || char === "'") && quote === null) {
      quote = char;
      continue;
    }
    if (char === quote) {
      quote = null;
      continue;
    }
    if (/\s/.test(char) && quote === null) {
      if (current) {
        args.push(current);
        current = "";
      }
      continue;
    }
    current += char;
  }

  if (current) args.push(current);
  return args;
}
